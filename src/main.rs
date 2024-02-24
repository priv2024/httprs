use clap::{value_parser, Parser};
use regex::bytes::Regex;
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, Stdin};

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

    let (tx, rx) = async_channel::bounded::<String>(config.tasks);
    let (output_tx, output_rx) = async_channel::bounded::<String>(1);

    // receive and write to stdout matched urls
    let output_handle = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Ok(url) = output_rx.recv().await {
            stdout
                .write_all(url.as_bytes())
                .await
                .expect("failed to write to stdout");
            stdout
                .write_u8(b'\n')
                .await
                .expect("failed to write new line to stdout");
        }
        output_rx.close();
    });

    // received input hosts and search for matches
    let mut handles = vec![];
    for _ in 0..config.tasks {
        let regexes = config.match_regexes.clone();
        let client = client.clone();
        let input_rx = rx.clone();
        let output_tx = output_tx.clone();

        handles.push(tokio::spawn(async move {
            while let Ok(host) = input_rx.recv().await {
                for url in get_url_variants(host) {
                    if process_url(&client, &url, &regexes).await {
                        // input matched
                        // stop processing further variants
                        output_tx.send(url).await.unwrap();
                        break;
                    }
                }
            }

            input_rx.close();
        }));
    }

    while let Some(line) = lines.next_line().await.unwrap() {
        tx.send(line).await.unwrap()
    }

    tx.close();

    for task in handles {
        task.await.unwrap();
    }

    output_tx.close();
    output_handle.await.unwrap();

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
