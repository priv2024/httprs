use reqwest::Client;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, Lines, Stdin};

const SCHEME_HTTPS: &str = "https://";
const SCHEME_HTTP: &str = "http://";

struct Config {
    /** Timeout in milliseconds */
    timeout: u64,

    /** Number of concurrent requests */
    threads: usize,
}

async fn process(
    mut lines: Lines<BufReader<Stdin>>,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Arc::new(
        Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_millis(config.timeout))
            .redirect(reqwest::redirect::Policy::none())
            .tcp_keepalive(None)
            .tcp_nodelay(true)
            .https_only(false)
            .user_agent("httprs/0.1.0")
            .build()
            .unwrap(),
    );

    let (input_tx, input_rx) = loole::bounded::<String>(config.threads);

    let mut tasks = vec![];
    for _ in 0..config.threads {
        let client = client.clone();
        let input_rx = input_rx.clone();

        tasks.push(tokio::spawn(async move {
            while let Ok(host) = input_rx.recv_async().await {
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
            input_rx.close();
        }))
    }

    while let Some(host) = lines.next_line().await? {
        input_tx.send_async(host).await.unwrap();
    }

    input_tx.close();

    for task in tasks {
        task.await.unwrap();
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let config = Config {
        timeout: 5000,
        threads: 600,
    };

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    process(reader.lines(), &config)
        .await
        .expect("error while processing input");
}
