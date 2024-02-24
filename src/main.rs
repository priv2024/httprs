use clap::{value_parser, Parser};
use regex::bytes::Regex;
use reqwest::Client;
use std::ops::Add;
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};

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

async fn process_host(client: &Client, host: &String, regexes: &Vec<Regex>) {
    let mut schemes = vec![];

    if host.starts_with("https://") || host.starts_with("http://") {
        schemes.push(None);
    } else {
        for scheme in ["https://", "http://"] {
            if host.ends_with(":80") && scheme == "https" {
                continue;
            } else if host.ends_with(":443") && scheme == "http" {
                continue;
            }
            schemes.push(Some(scheme));
        }
    }

    for scheme in schemes {
        let url = match scheme {
            None => String::from(host),
            Some(scheme) => String::from(scheme).add(host),
        };

        match process_url(client, &url, regexes).await {
            true => {
                println!("{}", url);
                break;
            }
            false => continue,
        }
    }
}

async fn process(
    mut lines: Lines<BufReader<Stdin>>,
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

    let (tx, rx) = async_channel::bounded(config.tasks);

    let mut handles = vec![];
    for _ in 0..config.tasks {
        let regexes = config.match_regexes.clone();
        let client = client.clone();
        let rx = rx.clone();

        handles.push(tokio::spawn(async move {
            while let Ok(host) = rx.recv().await {
                process_host(&client, &host, &regexes).await
            }

            rx.close();
        }));
    }

    while let Some(line) = lines.next_line().await.unwrap() {
        tx.send(line).await.unwrap()
    }

    tx.close();

    for task in handles {
        task.await.unwrap();
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
