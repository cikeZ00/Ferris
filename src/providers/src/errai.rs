use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use std::fs;

pub async fn errai(name: &str, season: u8, episode: u16, language: &str) -> Result<()> {
    // Load cookie from data/config.ini
    let mut cookies = String::new();

    if let Ok(cookie_data) = fs::read_to_string("data/config.ini") {
        for line in cookie_data
            .lines()
            .filter(|line| line.starts_with("errai_cookie"))
        {
            if let Some(cookie_value) = line.splitn(2, '=').nth(1) {
                cookies.push_str(cookie_value.trim());
            }
        }
    }

    if cookies.is_empty() {
        println!("Please paste your Errai cookie into data/config.ini and run the program again.");
        return Ok(());
    }

    let client = Client::new();

    let mut search_term = jikan_resolve_title(name).await?;

    // Set headers
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&cookies).unwrap());
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/85.0.4183.121 Safari/537.36"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://www.erai-raws.info/"),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));

    // TODO: Find a better way to handle searching for seasons
    if season > 1 {
        search_term.push_str(&format!(" {}", season));
    }

    println!("Searching for: {}", search_term);

    // We hijack the main websites search functionality to get the search results
    let url = format!(
        "https://www.erai-raws.info/?s={}",
        search_term.replace(" ", "+")
    );
    let res = client.get(&url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        // Find articles by parsing the <article> tag and extracting <h2> and <a>
        for article in document.find(Name("article")) {
            let title = article.find(Name("h2")).next().and_then(|h2| {
                h2.find(Name("a"))
                    .next()
                    .map(|a| (a.text(), a.attr("href").unwrap_or("#").to_string()))
            });

            if let Some((article_title, article_link)) = title {
                println!("Title: {} | Link: {}", article_title, article_link);

                let sub_dir = match fetch_sub_url(&client, &article_link, &headers).await {
                    Ok(url) => url,
                    Err(e) => {
                        println!("Failed to fetch article details: {}", e);
                        continue;
                    }
                };

                // Fetch subtitles
                let subtitle = match fetch_subtitle(&client, &sub_dir, &headers).await {
                    Ok(sub) => sub,
                    Err(e) => {
                        println!("Failed to fetch subtitles: {}", e);
                        continue;
                    }
                };

                println!("Subtitle: {}", subtitle);
            }
        }
    } else {
        println!("Failed to fetch search results. Status: {}", res.status());
    }

    Ok(())
}

// Fetch the sub url from the main website
async fn fetch_sub_url(client: &Client, url: &str, headers: &HeaderMap) -> Result<String> {
    let res = client.get(url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        if let Some(menu0) = document.find(Attr("id", "menu0")).next() {
            for row in menu0.find(Name("a")) {
                let text = row.text();
                if text == "Subtitles" {
                    let link = row.attr("href").unwrap_or("#").to_string();
                    return Ok(link);
                }
            }
        } else {
            println!("menu0 not found on the page.");
        }
    } else {
        println!("Failed to fetch article page. Status: {}", res.status());
    }

    Ok(String::new())
}

// Fetch the actual subtitles from the subs directory page
async fn fetch_subtitle(client: &Client, url: &str, headers: &HeaderMap) -> Result<String> {
    let res = client.get(url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        if let Some(dirlist) = document.find(Attr("id", "directory-listing")).next() {
            for row in dirlist.find(Name("li")) {
                let link = row.attr("data-href").unwrap_or("#").to_string();
                let name = row.attr("data-name").unwrap_or("#").to_string();
                println!("Name: {} | Link: {}", name, link);
            }
        } else {
            println!("Subtitles not found on the page.");
        }
    } else {
        println!("Failed to fetch subtitles page. Status: {}", res.status());
    }

    Ok(String::new())
}

async fn jikan_resolve_title(title: &str) -> Result<String> {
    let url = format!("https://api.jikan.moe/v4/anime?q={}&limit=3", title);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body).expect("Failed to parse JSON");

        if let Some(anime) = json["data"][0]["title"].as_str() {
            println!("Romaji: {}", anime);
            return Ok(anime.to_string());
        }
    } else {
        println!("Failed to fetch anime details. Status: {}", res.status());
    }

    Ok(String::new())
}
