use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs::File;
use clap::Parser;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use csv::Writer;
use regex::Regex;

struct ScrapedData {
    selector_name: String,
    values: Vec<String>,
}

/// Fast, non-interactive web scraper. Supports multiple URLs and repeatable CSS selectors.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// One or more URLs to scrape (results merged into one CSV)
    #[clap(required = true)]
    urls: Vec<String>,

    /// Selector in "name:css_selector" format. Repeat for each element.
    /// Example: --selector "links:a.nav" --selector "hovered:a:hover"
    #[clap(short, long = "selector", required = true)]
    selectors: Vec<String>,

    /// Enable pagination (appends ?page=N or &page=N to each URL)
    #[clap(short, long)]
    paginate: bool,

    /// Max pages per URL when --paginate is active
    #[clap(long, default_value = "10")]
    max_pages: usize,

    /// Remove duplicate rows from results
    #[clap(short, long)]
    deduplicate: bool,

    /// Output CSV filename
    #[clap(short, long, default_value = "scraped_data.csv")]
    output: String,

    /// Mark as JavaScript-heavy site (accepted for completeness; no effect)
    #[clap(long)]
    js_heavy: bool,
}

fn parse_selector(raw: &str) -> Result<(String, String), Box<dyn Error>> {
    let mut parts = raw.splitn(2, ':');
    let name = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Selector '{}' has an empty name before the colon", raw))?
        .trim()
        .to_string();
    let selector = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!(
            "Selector '{}' is missing a CSS selector after the colon. Expected: name:css_selector",
            raw
        ))?
        .trim()
        .to_string();
    Ok((name, selector))
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();

    let mut selectors: HashMap<String, String> = HashMap::new();
    for raw in &args.selectors {
        let (name, css) = parse_selector(raw)?;
        selectors.insert(name, css);
    }

    let client = Client::builder()
        .user_agent(get_random_user_agent())
        .build()?;

    let max_pages = if args.paginate { args.max_pages } else { 1 };
    let mut all_data: Vec<ScrapedData> = Vec::new();

    for url in &args.urls {
        println!("--- Scraping URL: {} ---", url);
        for page_num in 1..=max_pages {
            let page_url = if page_num == 1 {
                url.clone()
            } else if url.contains('?') {
                format!("{}&page={}", url, page_num)
            } else {
                format!("{}?page={}", url, page_num)
            };

            println!("Scraping page {} of {}: {}", page_num, max_pages, page_url);
            let html_content = fetch_html_with_retry(&client, &page_url, 3)?;
            let document = Html::parse_document(&html_content);
            let page_data = scrape_data(&document, &selectors)?;

            if page_data.is_empty() {
                println!("No data found on page {}. Stopping pagination.", page_num);
                break;
            }

            merge_scraped_data(&mut all_data, page_data);

            if page_num < max_pages {
                println!("Waiting before next request...");
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    let final_data = if args.deduplicate {
        deduplicate_data(all_data)
    } else {
        all_data
    };

    save_to_csv(&final_data, &args.output)?;
    println!("Scraping completed successfully! Data saved to '{}'", args.output);
    Ok(())
}

fn fetch_html_with_retry(client: &Client, url: &str, max_retries: usize) -> Result<String, Box<dyn Error>> {
    let mut attempts = 0;
    let mut last_error = None;

    while attempts < max_retries {
        println!("Fetching HTML from {}... (attempt {}/{})", url, attempts + 1, max_retries);

        match client.get(url).send() {
            Ok(response) => {
                if response.status().is_success() {
                    let html_content = response.text()?;
                    println!("HTML fetched successfully ({} bytes)", html_content.len());
                    return Ok(html_content);
                } else {
                    println!("Received status code {}, retrying...", response.status());
                }
            },
            Err(e) => {
                println!("Request error: {}, retrying...", e);
                last_error = Some(Box::new(e) as Box<dyn Error>);
            }
        }

        attempts += 1;
        if attempts < max_retries {
            let backoff_time = std::time::Duration::from_secs(2u64.pow(attempts as u32));
            println!("Waiting for {} seconds before retry", backoff_time.as_secs());
            std::thread::sleep(backoff_time);
        }
    }

    Err(last_error.unwrap_or_else(|| "Maximum retries exceeded".into()))
}

fn scrape_data(document: &Html, selectors: &HashMap<String, String>) -> Result<Vec<ScrapedData>, Box<dyn Error>> {
    let mut result: Vec<ScrapedData> = Vec::new();

    for (selector_name, selector_value) in selectors {
        let selector = match Selector::parse(selector_value) {
            Ok(sel) => sel,
            Err(_) => return Err(format!("Invalid selector: {}", selector_value).into()),
        };

        let mut values: Vec<String> = Vec::new();

        for element in document.select(&selector) {
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
            let cleaned_text = clean_text(&text);

            if !cleaned_text.is_empty() {
                values.push(cleaned_text);
            } else if let Some(href) = element.value().attr("href") {
                values.push(href.to_string());
            }
        }

        let values_len = values.len();

        if !values.is_empty() {
            result.push(ScrapedData {
                selector_name: selector_name.clone(),
                values,
            });
            println!("Found {} elements matching selector '{}'", values_len, selector_name);
        } else {
            println!("No elements found for selector '{}'", selector_name);
        }
    }

    Ok(result)
}

fn save_to_csv(data: &[ScrapedData], filename: &str) -> Result<(), Box<dyn Error>> {
    let file = File::create(filename)?;
    let mut writer = Writer::from_writer(file);

    let max_values = data.iter().map(|d| d.values.len()).max().unwrap_or(0);

    let headers: Vec<String> = data.iter().map(|d| d.selector_name.clone()).collect();
    writer.write_record(&headers)?;

    for i in 0..max_values {
        let row: Vec<String> = data
            .iter()
            .map(|d| d.values.get(i).cloned().unwrap_or_default())
            .collect();
        writer.write_record(&row)?;
    }

    writer.flush()?;
    Ok(())
}

fn clean_text(text: &str) -> String {
    let cleaned = text.trim().to_string();
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(&cleaned, " ").to_string()
}

fn get_random_user_agent() -> &'static str {
    let user_agents = [
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.1.1 Safari/605.1.15",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:89.0) Gecko/20100101 Firefox/89.0",
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/92.0.4515.107 Safari/537.36",
        "Mozilla/5.0 (iPhone; CPU iPhone OS 14_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/14.0 Mobile/15E148 Safari/604.1",
    ];

    let random_index = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize % user_agents.len();

    user_agents[random_index]
}

fn deduplicate_data(data: Vec<ScrapedData>) -> Vec<ScrapedData> {
    let mut result: Vec<ScrapedData> = Vec::new();
    let mut seen_items = HashSet::new();

    for item in &data {
        let mut identifier = item.selector_name.clone();
        for value in &item.values {
            identifier.push_str(value);
        }

        if !seen_items.contains(&identifier) {
            seen_items.insert(identifier);
            result.push(ScrapedData {
                selector_name: item.selector_name.clone(),
                values: item.values.clone(),
            });
        }
    }

    println!("Removed {} duplicate entries", data.len() - result.len());
    result
}

fn merge_scraped_data(base: &mut Vec<ScrapedData>, new: Vec<ScrapedData>) {
    for new_item in new {
        if let Some(existing) = base.iter_mut().find(|b| b.selector_name == new_item.selector_name) {
            existing.values.extend(new_item.values);
        } else {
            base.push(new_item);
        }
    }
}
