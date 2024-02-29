use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use colored::{Color, Colorize};
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
        help_heading = "Optimizations ⚙️",
        help = "request duration threshold in milliseconds"
    )]
    timeout: u64,

    /// Number of concurrent requests
    #[arg(
        short = 't',
        long = "tasks",
        default_value_t = 60,
        help_heading = "Rate-Limit 🐌",
        help = "number of concurrent requests"
    )]
    tasks: usize,

    /// Regular expression to match
    #[arg(
        short = 'r',
        long = "match-regex",
        help_heading = "Matchers 🔍",
        help = "path to a list of regex patterns"
    )]
    match_regexes_path: Option<PathBuf>,
}

async fn parse_regexes(path: &PathBuf) -> Vec<Regex> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => text
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line| Regex::new(line).expect("invalid regex"))
            .collect(),
        Err(err) => panic!("failed to read {:?} : {}", path, err),
    }
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

async fn process_url(client: &Client, url: &String, regexes: &Vec<Regex>) -> Option<Vec<String>> {
    match client.get(url).send().await {
        Err(_) => None,
        Ok(res) => match regexes.is_empty() {
            true => Some(vec![]),
            false => match res.bytes().await {
                Err(_) => None,
                Ok(bytes) => {
                    let bytes = bytes.as_ref();
                    let matches = regexes
                        .iter()
                        .filter_map(|re| re.captures(bytes))
                        .filter_map(|caps| caps.get(1).or(caps.get(0)))
                        .map(|m| m.as_bytes())
                        .map(|value| String::from_utf8_lossy(value).to_string())
                        .collect::<Vec<String>>();
                    return if !matches.is_empty() {
                        Some(matches)
                    } else {
                        None
                    };
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

    let regexes = if let Some(path) = &config.match_regexes_path {
        parse_regexes(&path).await
    } else {
        vec![]
    };

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
            .rev()
            .collect();

        for index in handle_finished_indices {
            handles.swap_remove(index);
        }

        let regexes = regexes.clone();
        let client = client.clone();

        handles.push(tokio::spawn(async move {
            for url in get_url_variants(host) {
                if let Some(matches) = process_url(&client, &url, &regexes).await {
                    if !matches.is_empty() {
                        println!("{} {}", url, matches.join(" ").color(Color::Cyan));
                    } else {
                        println!("{}", url);
                    }
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
