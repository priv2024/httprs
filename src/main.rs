use std::sync::Arc;

use clap::{value_parser, Parser};
use regex::bytes::Regex;
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

#[derive(Parser)]
#[command(version, about = "🧨 http toolkit that allows probing many hosts.")]
struct Config {
    /// Timeout in milliseconds
    #[arg(
        short = 'T',
        long = "timeout",
        default_value_t = 6000,
        help_heading = "Optimizations ⚙️"
    )]
    timeout: u64,

    /// Number of concurrent requests
    #[arg(
        short = 't',
        long = "tasks",
        default_value_t = 60,
        help_heading = "Rate-Limit 🐌"
    )]
    tasks: usize,

    /// Regular expression to match
    #[arg(
        short = 'r',
        long = "match-regex",
        help_heading = "Matchers 🔍",
        value_parser = value_parser!(Regex)
    )]
    match_regexes: Vec<Regex>,
}

fn get_url_variants(host: String) -> Vec<String> {
    return if host.starts_with("https://") || host.starts_with("http://") {
        vec![host]
    } else {
        vec![
            String::from("https://") + host.as_str(),
            String::from("http://") + host.as_str(),
        ]
    };
}

async fn process_url(client: &Client, url: &String, regexes: &Vec<Regex>) -> bool {
    match client.get(url).send().await {
        Err(_) => false,
        Ok(res) => match regexes.is_empty() {
            true => true,
            false => match res.bytes().await {
                Err(_) => false,
                Ok(bytes) => {
                    let bytes = bytes.as_ref();
                    for regex in regexes {
                        if regex.is_match(bytes) {
                            return true;
                        }
                    }
                    return false;
                }
            },
        },
    }
}

async fn process(
    mut host_lines: Lines<BufReader<Stdin>>,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_millis(config.timeout))
        .redirect(reqwest::redirect::Policy::none())
        .tcp_keepalive(None)
        .tcp_nodelay(true)
        .https_only(false)
        .pool_max_idle_per_host(0)
        .user_agent("httprs/0.1.0")
        .build()
        .unwrap();

    let semaphore = Arc::new(Semaphore::new(config.tasks));
    let mut handles: Vec<JoinHandle<()>> = vec![];

    while let Some(host) = host_lines.next_line().await.unwrap() {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("failed to acquire permit");

        let handle_finished_indices: Vec<usize> = handles
            .iter()
            .enumerate()
            .filter(|(_, handle)| handle.is_finished())
            .map(|(index, _)| index)
            .collect();

        for index in handle_finished_indices {
            handles.swap_remove(index);
        }

        let client = client.clone();
        let regexes = config.match_regexes.clone();

        handles.push(tokio::spawn(async move {
            for url in get_url_variants(host) {
                if process_url(&client, &url, &regexes).await {
                    println!("{}", url);
                    break;
                }
            }

            drop(permit);
        }));
    }

    for handle in handles {
        handle.await.expect("fatal error in a task")
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let config = Config::parse();

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    process(reader.lines(), &config)
        .await
        .expect("error while processing input");
}
