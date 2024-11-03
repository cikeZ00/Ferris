use anyhow::anyhow;
use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration, Instant};

#[derive(Debug, Clone)]
struct Season {
    title: String,
    season: i32,
    part: i32,
    episodes: i32,
}

// Caching
#[derive(Clone)]
struct CacheEntry {
    value: Value,
    expiry: Instant,
}

#[derive(Clone)]
struct Cache {
    data: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl Cache {
    fn new(ttl: Duration) -> Self {
        Cache {
            data: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    fn get(&self, key: &str) -> Option<Value> {
        let data = self.data.lock().unwrap();
        if let Some(entry) = data.get(key) {
            if entry.expiry > Instant::now() {
                return Some(entry.value.clone());
            }
        }
        None
    }

    fn set(&self, key: String, value: Value) {
        let mut data = self.data.lock().unwrap();
        let entry = CacheEntry {
            value,
            expiry: Instant::now() + self.ttl,
        };
        data.insert(key, entry);
    }
}

pub async fn errai(name: &str, es: &str, language: &str) -> Result<()> {
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
    let anime = jikan_fetch_anime(name, es).await?;

    if anime.0 == "None" {
        println!("Anime not found.");
        return Ok(());
    }

    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(&cookies).unwrap());
    headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://www.erai-raws.info/"),
    );
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml"),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));

    println!("Searching for: {}", anime.0.clone());

    let url = format!(
        "https://www.erai-raws.info/?s={}",
        anime.0.replace(" ", "+")
    );
    let res = client.get(&url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        let mut series_titles_list = Vec::new();
        let mut title_pages = Vec::new();

        for article in document.find(Name("article")) {
            let title = article.find(Name("h2")).next().and_then(|h2| {
                h2.find(Name("a"))
                    .next()
                    .map(|a| (a.text(), a.attr("href").unwrap_or("#").to_string()))
            });

            if let Some((article_title, article_link)) = title {
                series_titles_list.push(article_title.clone());
                title_pages.push((article_title.clone(), article_link.clone()));
            }
        }

        let link = title_pages
            .iter()
            .find(|(title, _)| title.to_string() == anime.0.clone());

        if let Some((_, link)) = link {
            let sub_dir = match fetch_sub_url(&client, link, &headers).await {
                Ok(url) => url,
                Err(e) => {
                    println!("Failed to fetch article details: {}", e);
                    return Ok(());
                }
            };

            if let Some((_, wanted_episode)) = extract_season_episode(es) {
                // Fetch subtitles
                let _subtitle =
                    match fetch_subtitle(&client, &sub_dir, &headers, wanted_episode, &language)
                        .await
                    {
                        Ok(sub) => sub,
                        Err(e) => {
                            println!("Failed to fetch subtitles: {}", e);
                            return Ok(());
                        }
                    };
            } else {
                println!("Failed to extract season and episode from input.");
                return Ok(());
            }
        }
    } else {
        println!("Failed to fetch search results. Status: {}", res.status());
    }

    Ok(())
}

fn extract_season_episode(input: &str) -> Option<(u32, u32)> {
    let re = Regex::new(r"(\d+)x(\d+)").unwrap();
    if let Some(caps) = re.captures(input) {
        let season = caps[1].parse::<u32>().ok()?;
        let episode = caps[2].parse::<u32>().ok()?;
        return Some((season, episode));
    }
    None
}

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

async fn fetch_subtitle(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    episode: u32,
    language: &str,
) -> Result<String> {
    let res = client.get(url).headers(headers.clone()).send().await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let document = Document::from(body.as_str());

        if let Some(dirlist) = document.find(Attr("id", "directory-listing")).next() {
            for row in dirlist.find(Name("li")) {
                let link = row.attr("data-href").unwrap_or("#").to_string();
                let name = row.attr("data-name").unwrap_or("#").to_string();
                if name != ".." && !link.ends_with(".7z") {
                    println!("Name: {} | Link: {}", name, link);
                }
            }
        } else {
            println!("Subtitles not found on the page.");
        }
    } else {
        println!("Failed to fetch subtitles page. Status: {}", res.status());
    }

    Ok(String::new())
}

async fn jikan_fetch_anime(title: &str, es: &str) -> Result<(String, u32)> {
    let cache = Cache::new(Duration::from_secs(3600)); // Cache TTL of 1 hour

    // We want to fetch the ID of the 1st season of the show
    let url = format!("https://api.jikan.moe/v4/anime?q={}&limit=1", title);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: Value = serde_json::from_str(&body).expect("Failed to parse JSON");

        let id = json["data"][0]["mal_id"].clone();
        println!("Fetched ID: {}", id);

        if let Some((wanted_season, wanted_episode)) = extract_season_episode(es) {
            println!("Wanted Season: {}", wanted_season);
            println!("Wanted Episode: {}", wanted_episode);

            // Here we build the Series object that includes all of the seasons of the show
            let series = build_full_series(id.as_i64().unwrap() as i32, &cache).await?;

            // Use merge_seasons to merge parts of the seasons
            let merged_series = merge_seasons(series);

            // Find the desired season in the merged series
            if let Some(season) = merged_series
                .iter()
                .find(|season| season.season == wanted_season as i32)
            {
                println!("Found season: {:?}", season);

                let mut remaining_episodes = wanted_episode;

                // Check if season is split into parts
                if remaining_episodes > season.episodes as u32 {
                    let next_part = merged_series.iter().find(|next_season| {
                        next_season.season == season.season && next_season.part == season.part + 1
                    });

                    if let Some(next_part) = next_part {
                        // Decrease remaining_episodes by the current season's episodes
                        remaining_episodes -= season.episodes as u32;
                        println!("Fetching next part: {:?}", next_part);
                        println!("Remaining episodes to find: {}", remaining_episodes);

                        if remaining_episodes <= next_part.episodes as u32 {
                            // Return the part 2's title and episode remainder
                            println!("In {}: Episode {}", next_part.title, remaining_episodes);
                            let episode_info: (String, u32) =
                                (next_part.title.clone(), remaining_episodes);
                            return Ok(episode_info);
                        } else {
                            println!(
                                "Not enough episodes in next part. Total episodes: {}",
                                next_part.episodes
                            );
                        }
                    } else {
                        println!("No next part found after part {}", season.part);
                    }
                } else {
                    // This season is not split into parts
                    let episode_info: (String, u32) = (season.title.clone(), remaining_episodes);
                    println!("In {}: Episode {}", season.title, remaining_episodes);
                    return Ok(episode_info);
                }
            } else {
                println!("Season {} not found", wanted_season);
            }
        } else {
            println!("Failed to parse season and episode.");
        }
    } else {
        println!("Failed to fetch anime details. Status: {}", res.status());
    }

    // Temporary until we implement custom Errors
    Ok(("None".to_string(), 0))
}

// Function to build a full series object with all sequels properly mapped
// TODO: Change result into a better fitting object instead of Vec
async fn build_full_series(id: i32, cache: &Cache) -> Result<Vec<Season>> {
    let mut series_full = Vec::new();
    let mut current_id = id;
    let mut season_number = 1;
    let mut part_number = 1;

    loop {
        let anime_data = jikan_fetch_anime_by_id(current_id, cache).await.unwrap();

        let episode_count = anime_data["data"]["episodes"].as_i64().unwrap_or(0) as i32;
        let title = anime_data["data"]["title"]
            .as_str()
            .unwrap_or("Unknown Title");

        let season_to_build = Season {
            title: title.to_string(), // Ensure it owns the title data
            season: season_number,
            part: part_number,
            episodes: episode_count,
        };
        series_full.push(season_to_build);

        // Fetch the next sequel ID, if any
        match jikan_fetch_related_sequel(current_id as i64, cache).await {
            Ok(next_id) if next_id != 0 => {
                // Check if the next season is a part of the current season
                let next_anime_data = jikan_fetch_anime_by_id(next_id, cache).await.unwrap();
                let next_title = next_anime_data["data"]["title"]
                    .as_str()
                    .unwrap_or("Unknown Title");

                if next_title.contains("2nd Season") && title.contains("2nd Season") {
                    part_number += 1;
                } else if next_title.contains("3rd Season") && title.contains("3rd Season") {
                    part_number += 1;
                } else {
                    season_number += 1;
                    part_number = 1;
                }

                current_id = next_id;
            }
            _ => {
                println!("No further sequels found. Series mapping complete.");
                break;
            }
        }

        sleep(Duration::from_secs(1)).await;
    }

    Ok(series_full)
}

async fn jikan_fetch_anime_by_id(id: i32, cache: &Cache) -> anyhow::Result<Value> {
    sleep(Duration::from_secs(1)).await;
    let cache_key = format!("anime_{}", id);
    if let Some(cached_value) = cache.get(&cache_key) {
        return Ok(cached_value);
    }

    let url = format!("https://api.jikan.moe/v4/anime/{}", id);
    let res = reqwest::get(&url)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    if res.status().is_success() {
        let body = res.text().await.map_err(|e| anyhow!(e.to_string()))?;
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;
        cache.set(cache_key, json.clone());
        Ok(json)
    } else {
        Err(anyhow!(
            "Failed to fetch anime details. Status: {}",
            res.status()
        ))
    }
}

async fn jikan_fetch_related_sequel(id: i64, cache: &Cache) -> Result<i32> {
    let cache_key = format!("relations_{}", id);
    if let Some(cached_value) = cache.get(&cache_key) {
        if let Some(sequel_id) = cached_value["data"]
            .as_array()
            .and_then(|arr| arr.iter().find(|relation| relation["relation"] == "Sequel"))
            .and_then(|relation| relation["entry"][0]["mal_id"].as_i64())
        {
            return Ok(sequel_id as i32);
        }
    }

    let url = format!("https://api.jikan.moe/v4/anime/{}/relations", id);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: Value = serde_json::from_str(&body).expect("Failed to parse JSON");
        cache.set(cache_key, json.clone());
        if let Some(sequel_id) = json["data"]
            .as_array()
            .and_then(|arr| arr.iter().find(|relation| relation["relation"] == "Sequel"))
            .and_then(|relation| relation["entry"][0]["mal_id"].as_i64())
        {
            return Ok(sequel_id as i32);
        }
    }

    Ok(0)
}

fn merge_seasons(series: Vec<Season>) -> Vec<Season> {
    let mut merged_series: Vec<Season> = Vec::new();
    let mut season_map: std::collections::HashMap<(i32, i32), Season> =
        std::collections::HashMap::new();

    for season in series {
        let key = (season.season, season.part);
        if let Some(existing_season) = season_map.get_mut(&key) {
            existing_season.episodes += season.episodes;
        } else {
            season_map.insert(key, season.clone());
            merged_series.push(season);
        }
    }

    // Sort the merged series by season number and part number to maintain order
    merged_series.sort_by(|a, b| a.season.cmp(&b.season).then_with(|| a.part.cmp(&b.part)));
    merged_series
}
