use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};
use std::fs::File;
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use csv::Writer;
use regex::Regex;
// HashSet is actually used in the deduplicate_data function
use std::collections::HashSet;

// Define a struct to store scraped data
struct ScrapedData {
    selector_name: String,
    values: Vec<String>,
}

// Removed the unused Product struct

fn main() -> Result<(), Box<dyn Error>> {
    println!("Web Scraper in Rust");
    println!("---------------------");

    // Get a random user agent
    let user_agent = get_random_user_agent();

    // Create HTTP client with custom user agent
    let client = Client::builder()
        .user_agent(user_agent)
        .build()?;

    // Get URL from user
    let url = get_user_input("Enter the URL to scrape: ")?;

    // Ask if pagination is needed
    let use_pagination = get_user_input("Do you want to use pagination? (y/n): ")?.to_lowercase();
    let max_pages = if use_pagination == "y" {
        get_user_input("Enter maximum number of pages to scrape: ")?.parse::<usize>()?
    } else {
        1
    };

    // Ask if the site is JavaScript-heavy
    let js_heavy = get_user_input("Is this a JavaScript-heavy site? (y/n): ")?.to_lowercase();
    let _is_js_heavy = js_heavy == "y"; 

    // Get selectors from user
    let mut selectors: HashMap<String, String> = HashMap::new();
    let num_selectors = get_user_input("How many elements do you want to scrape? ")?.parse::<usize>()?;

    // Loop to collect multiple selectors
    for i in 1..=num_selectors {
        let selector_name = get_user_input(&format!("Enter name for selector {}: ", i))?;
        let selector_value = get_user_input(&format!("Enter CSS selector {}: ", i))?;
        selectors.insert(selector_name, selector_value);
    }

    // Initialize the vector that will store all scraped data
    let mut all_data: Vec<ScrapedData> = Vec::new();

    // Scrape with pagination if requested
    for page_num in 1..=max_pages {
        let page_url = if page_num == 1 {
            url.clone()
        } else {
            // Basic pagination pattern - modify as needed for specific sites
            if url.contains('?') {
                format!("{}&page={}", url, page_num)
            } else {
                format!("{}?page={}", url, page_num)
            }
        };

        println!("Scraping page {} of {}: {}", page_num, max_pages, page_url);

        // Fetch the HTML content with retry logic
        let html_content = fetch_html_with_retry(&client, &page_url, 3)?;

        // Parse the HTML document
        let document = Html::parse_document(&html_content);

        // Scrape the data using the selectors
        let page_data = scrape_data(&document, &selectors)?;

        // If no data was found, we've likely reached the end of pagination
        if page_data.is_empty() {
            println!("No data found on page {}. Stopping pagination.", page_num);
            break;
        }

        // Add the page data to our collection
        all_data.extend(page_data);

        // Be nice to the server - add a small delay between requests
        if page_num < max_pages {
            println!("Waiting before next request...");
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    // Deduplicate data if requested
    let deduplicate = get_user_input("Do you want to remove duplicate entries? (y/n): ")?.to_lowercase();
    let final_data = if deduplicate == "y" {
        deduplicate_data(all_data)
    } else {
        all_data
    };

    // Ask for output filename
    let output_file = get_user_input("Enter output filename (default: scraped_data.csv): ")?;
    let output_file = if output_file.trim().is_empty() {
        "scraped_data.csv".to_string()
    } else {
        output_file
    };

    // Save the data to CSV
    save_to_csv(&final_data, &output_file)?;
    println!("Scraping completed successfully! Data saved to '{}'", output_file);

    Ok(())
}

// Function to get user input (returns ownership of the String)
fn get_user_input(prompt: &str) -> Result<String, Box<dyn Error>> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    // Remove trailing newline
    if input.ends_with('\n') {
        input.pop();
    }
    if input.ends_with('\r') {
        input.pop();
    }

    Ok(input)
}

// Function to fetch HTML content with retry logic
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

// Function to scrape data using selectors
fn scrape_data(document: &Html, selectors: &HashMap<String, String>) -> Result<Vec<ScrapedData>, Box<dyn Error>> {
    let mut result: Vec<ScrapedData> = Vec::new();

    // Iterate through each selector
    for (selector_name, selector_value) in selectors {
        // Parse the selector
        let selector = match Selector::parse(selector_value) {
            Ok(sel) => sel,
            Err(_) => return Err(format!("Invalid selector: {}", selector_value).into()),
        };

        // Select elements and extract text
        let mut values: Vec<String> = Vec::new();

        // Loop through all matching elements
        for element in document.select(&selector) {
            // Extract text content
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();

            // Clean the text
            let cleaned_text = clean_text(&text);

            // If text is not empty, add it to values
            if !cleaned_text.is_empty() {
                values.push(cleaned_text);
            } else if let Some(href) = element.value().attr("href") {
                // If text is empty but element has href attribute, use that
                values.push(href.to_string());
            }
        }

        // Get the length before moving values
        let values_len = values.len();

        // Only add valid data
        if !values.is_empty() {
            // Create ScrapedData struct and add to result
            result.push(ScrapedData {
                selector_name: selector_name.clone(),
                values, // values is moved here
            });
            println!("Found {} elements matching selector '{}'", values_len, selector_name);
        } else {
            println!("No elements found for selector '{}'", selector_name);
        }
    }

    Ok(result)
}

// Save data to CSV
fn save_to_csv(data: &[ScrapedData], filename: &str) -> Result<(), Box<dyn Error>> {
    // Create CSV file
    let file = File::create(filename)?;
    let mut writer = Writer::from_writer(file);

    // Determine the maximum number of values across all selectors
    let max_values = data.iter().map(|d| d.values.len()).max().unwrap_or(0);

    // Write header row
    let mut headers: Vec<String> = Vec::new();
    for scraped_data in data {
        headers.push(scraped_data.selector_name.clone());
    }
    writer.write_record(&headers)?;

    // Write data rows
    for i in 0..max_values {
        let mut row: Vec<String> = Vec::new();
        for scraped_data in data {
            if i < scraped_data.values.len() {
                row.push(scraped_data.values[i].clone());
            } else {
                row.push(String::new());
            }
        }
        writer.write_record(&row)?;
    }

    // Flush and close the writer
    writer.flush()?;
    Ok(())
}

// Clean text
fn clean_text(text: &str) -> String {
    // Remove extra whitespace
    let cleaned = text.trim().to_string();
    // Replace multiple spaces with a single space
    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(&cleaned, " ").to_string()
}

// Get a random user agent
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

// Function to deduplicate data
fn deduplicate_data(data: Vec<ScrapedData>) -> Vec<ScrapedData> {
    let mut result: Vec<ScrapedData> = Vec::new();
    let mut seen_items = HashSet::new();

    for item in &data {
        // Create a unique identifier for this data item
        let mut identifier = item.selector_name.clone();
        for value in &item.values {
            identifier.push_str(value);
        }

        // Check if we've seen this item before
        if !seen_items.contains(&identifier) {
            seen_items.insert(identifier);
            // Clone the item since we only have a reference
            result.push(ScrapedData {
                selector_name: item.selector_name.clone(),
                values: item.values.clone(),
            });
        }
    }

    println!("Removed {} duplicate entries", data.len() - result.len());
    result
}
