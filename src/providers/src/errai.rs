use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use std::fs;

// Jikan related API endpoint: https://api.jikan.moe/v4/anime/{id}/relations
pub async fn errai(name: &str, es: &str, _language: &str) -> Result<()> {
    // Load cookie from data/config.ini
    let mut cookies = String::new();

    jikan_fetch_related_sequel(9919).await?;

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
    let search_term = jikan_fetch_anime(name, es).await?;

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

    println!("Searching for: {}", search_term);

    // We hijack the main website's search functionality to get the search results
    let url = format!(
        "https://www.erai-raws.info/?s={}",
        search_term.replace(" ", "+")
    );
    let res = client.get(&url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        let mut series_titles_list = Vec::new();
        let mut title_pages = Vec::new();

        // Find articles by parsing the <article> tag and extracting <h2> and <a>
        for article in document.find(Name("article")) {
            let title = article.find(Name("h2")).next().and_then(|h2| {
                h2.find(Name("a"))
                    .next()
                    .map(|a| (a.text(), a.attr("href").unwrap_or("#").to_string()))
            });

            if let Some((article_title, article_link)) = title {
                // Push the title to the list
                series_titles_list.push(article_title.clone());

                // push title with link to title_pages
                title_pages.push((article_title.clone(), article_link.clone()));
            }
        }

        let link = "TODO";
        let sub_dir = match fetch_sub_url(&client, link, &headers).await {
            Ok(url) => url,
            Err(e) => {
                println!("Failed to fetch article details: {}", e);
                return Ok(());
            }
        };

        // Fetch subtitles
        let _subtitle = match fetch_subtitle(&client, &sub_dir, &headers).await {
            Ok(sub) => sub,
            Err(e) => {
                println!("Failed to fetch subtitles: {}", e);
                return Ok(());
            }
        };
    } else {
        println!("Failed to fetch search results. Status: {}", res.status());
    }

    Ok(())
}

// Function to extract the season from the input format (e.g., "2x18")
fn extract_season_episode(input: &str) -> Option<(u32, u32)> {
    let re = Regex::new(r"(\d+)x(\d+)").unwrap();
    if let Some(caps) = re.captures(input) {
        let season = caps[1].parse::<u32>().ok()?;
        let episode = caps[2].parse::<u32>().ok()?;
        return Some((season, episode));
    }
    None
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

async fn jikan_fetch_anime(title: &str, es: &str) -> Result<String> {
    let url = format!("https://api.jikan.moe/v4/anime?q={}&limit=3", title);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body).expect("Failed to parse JSON");

        let mut current_season = 1;
        let mut current_episodes: i32;

        let id = json["data"][0]["mal_id"].clone();
        println!("Fetched ID: {}", id);

        // Separate season and episode from input
        if let Some((season, episode_count)) = extract_season_episode(es) {
            let episodes = json["data"][0]["episodes"].clone();
            println!("Wanted Season: {}", season);
            println!("Wanted Episode: {}", episodes);

            // -----------------------------------------------------------------------------------------------FIX THIS------------------------------------------------------------------------------------------

            if episode_count > episodes.as_i64().unwrap() as u32 {
                current_season += 1;

                let seq = jikan_fetch_related_sequel(id.as_i64().unwrap());
                println!("Idk: {:?}", seq.await);
            }
        } else {
            println!("Failed to parse season and episode.");
        }

        // if let Some(anime) = json["data"][0]["title"].as_str() {
        // println!("Romaji: {}", anime);
        // return Ok(anime.to_string());
        // }

        //println!("Anime: {:?}", json.to_string());
    } else {
        println!("Failed to fetch anime details. Status: {}", res.status());
    }

    Ok(String::new())
}

async fn jikan_fetch_related_sequel(mal_id: i64) -> Result<String> {
    let url = format!("https://api.jikan.moe/v4/anime/{}/relations", mal_id);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: serde_json::Value = serde_json::from_str(&body).expect("Failed to parse JSON");

        if let Some(relations) = json["data"].as_array() {
            for relation in relations {
                //println!("Relation: {:?}", relation);

                // TODO: Check for type to ensure its anime

                if let Some("Sequel") = relation.get("relation").and_then(|r| r.as_str()) {
                    if let Some(entries) = relation.get("entry").and_then(|e| e.as_array()) {
                        if let Some(mal_id) = entries
                            .get(0)
                            .and_then(|entry| entry.get("mal_id"))
                            .and_then(|id| id.as_i64())
                        {
                            println!("Entry_id: {:?}", mal_id);
                        }
                    }
                }
            }
        }
    } else {
        println!("Failed to fetch anime details. Status: {}", res.status());
    }

    Ok(String::new())
}
