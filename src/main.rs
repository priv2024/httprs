use clap::Parser;
use reqwest::Client;
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};

const SCHEME_HTTPS: &str = "https://";
const SCHEME_HTTP: &str = "http://";

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
    tasks: usize
}

async fn process_host(client: &Client, host: &String) {
    let mut https_url = String::from(SCHEME_HTTPS);
    https_url.push_str(host.as_str());
    if let Ok(_res) = client.get(&https_url).send().await {
        println!("{}{}", SCHEME_HTTPS, host);
        return;
    }

    let mut http_url = String::from(SCHEME_HTTP);
    http_url.push_str(host.as_str());
    if let Ok(_res) = client.get(&http_url).send().await {
        println!("{}{}", SCHEME_HTTP, host);
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
        let client = client.clone();
        let rx = rx.clone();

        handles.push(tokio::spawn(async move {
            while let Ok(host) = rx.recv().await {
                process_host(&client, &host).await
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
