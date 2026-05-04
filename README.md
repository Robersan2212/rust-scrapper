# rust-scrapper

A command-line web scraper written in Rust that extracts structured data from websites using CSS selectors and exports results to CSV.

## Overview

Manually collecting data from websites is tedious, error-prone, and does not scale. `rust-scrapper` solves this by letting you define exactly what you want — through CSS selectors — and running the full fetch-parse-export pipeline without human intervention.

**What it does:**
- Accepts one or more URLs and any number of named CSS selectors as CLI arguments
- Fetches page content with automatic retries and exponential backoff on failure
- Parses the HTML and extracts all matching elements per selector
- Optionally follows pagination by appending `?page=N` query parameters
- Deduplicates results and exports everything to a single CSV file

**The problem it solves:**

Researchers, analysts, and developers frequently need structured data from websites that expose no API. The usual options are writing a one-off script per site, paying for a third-party scraping service, or copying data by hand. `rust-scrapper` provides a reusable, configurable tool that handles the networking, parsing, deduplication, and file export layers — so you only need to know the CSS selectors for the data you want.

**Why Rust:**

This project was built as a deliberate deep-dive into Rust. Rather than a toy exercise, the goal was to build something genuinely useful while learning Rust's ownership model, type system, and crate ecosystem. Rust was a natural fit for this use case: memory safety without a garbage collector means long-running scrape jobs do not accumulate memory pressure, and the strong type system surfaces configuration errors at compile time rather than mid-run against a live site.

**Usage:**

```bash
# Single URL, single selector
rust-scrapper https://example.com --selector "heading:h1"

# Multiple URLs with multiple selectors, pagination, deduplication
rust-scrapper https://site-a.com https://site-b.com \
  --selector "title:h2.product-title" \
  --selector "price:span.price" \
  --paginate --max-pages 5 \
  --deduplicate \
  --output products.csv

# Full help
rust-scrapper --help
```

## Development Environment

- Rust (edition 2024)
- Git

**Dependencies:**

| Crate | Version | Purpose |
|---|---|---|
| reqwest | 0.11 (blocking) | HTTP client |
| scraper | 0.18.1 | HTML parsing and CSS selector engine |
| clap | 3.2.25 (derive) | CLI argument parsing |
| csv | 1.3 | CSV export |
| regex | 1.10.2 | Text cleanup |
| log / env_logger | 0.4 / 0.10 | Structured logging |

## Useful Websites

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust Playground](https://play.rust-lang.org/?version=stable&mode=debug&edition=2024)
- [Rust Community](https://www.rust-lang.org/community)
- [r/rust](https://www.reddit.com/r/rust/)
- [Rust Web Scraping — ZenRows](https://www.zenrows.com/blog/rust-web-scraping)
- [Web Scraping in Rust with Reqwest — ScrapingBee](https://www.scrapingbee.com/blog/web-scraping-rust/)

## Future Work

- Headless browser integration for JavaScript-rendered pages
- Concurrent multi-URL fetching with Tokio
- Configurable pagination strategies (next-page link selectors, offset-based)
- Output formats beyond CSV (JSON, SQLite)
