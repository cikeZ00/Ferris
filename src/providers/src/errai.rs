use anyhow::anyhow;
use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use serde_json::Value;
use std::fs;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
struct Season {
    title: String, // Change to String to own the title data
    id: i32,
    season: i32,
    episodes: i32,
}

// Jikan-related API endpoint: https://api.jikan.moe/v4/anime/{id}/relations
pub async fn errai(name: &str, es: &str, _language: &str) -> Result<()> {
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

            // Fetch subtitles
            let _subtitle = match fetch_subtitle(&client, &sub_dir, &headers).await {
                Ok(sub) => sub,
                Err(e) => {
                    println!("Failed to fetch subtitles: {}", e);
                    return Ok(());
                }
            };
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

async fn jikan_fetch_anime(title: &str, es: &str) -> Result<(String, u32)> {
    let url = format!("https://api.jikan.moe/v4/anime?q={}&limit=3", title);
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
            let series = build_full_series(id.as_i64().unwrap() as i32).await?;

            // Use merge_seasons to merge parts of the seasons
            let merged_series = merge_seasons(series);

            println!("Full series: {:?}", merged_series);

            // Find the desired season in the merged series
            if let Some(season) = merged_series
                .iter()
                .find(|season| season.season == wanted_season as i32)
            {
                println!("Found season: {:?}", season);

                let mut remaining_episodes = wanted_episode;

                // Check if season is split into parts
                if remaining_episodes > season.episodes as u32 {
                    let next_season = merged_series
                        .iter()
                        .find(|next_season| next_season.season == season.season + 1);

                    if let Some(next_season) = next_season {
                        // Decrease remaining_episodes by the current season's episodes
                        remaining_episodes -= season.episodes as u32;
                        println!("Fetching next season: {:?}", next_season);
                        println!("Remaining episodes to find: {}", remaining_episodes);

                        if remaining_episodes <= next_season.episodes as u32 {
                            // Return the part 2's title and episode remainder
                            println!("In {}: Episode {}", next_season.title, remaining_episodes);
                            let episode_info: (String, u32) =
                                (next_season.title.clone(), remaining_episodes);
                            return Ok(episode_info);
                        } else {
                            println!(
                                "Not enough episodes in next season. Total episodes: {}",
                                next_season.episodes
                            );
                        }
                    } else {
                        println!("No next season found after season {}", season.season);
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
async fn build_full_series(id: i32) -> Result<Vec<Season>> {
    let mut series_full = Vec::new();
    let mut current_id = id;
    let mut season_number = 1;

    loop {
        let anime_data = jikan_fetch_anime_by_id(current_id).await.unwrap();

        let episode_count = anime_data["data"]["episodes"].as_i64().unwrap_or(0) as i32;
        let title = anime_data["data"]["title"]
            .as_str()
            .unwrap_or("Unknown Title");

        let season_to_build = Season {
            title: title.to_string(), // Ensure it owns the title data
            id: current_id,
            season: season_number,
            episodes: episode_count,
        };
        series_full.push(season_to_build);

        // Fetch the next sequel ID, if any
        match jikan_fetch_related_sequel(current_id as i64).await {
            Ok(next_id) if next_id != 0 => {
                season_number += 1;
                current_id = next_id;
            }
            _ => {
                println!("No further sequels found. Series mapping complete.");
                break;
            }
        }

        sleep(Duration::from_secs(2)).await;
    }

    Ok(series_full)
}

async fn jikan_fetch_anime_by_id(id: i32) -> anyhow::Result<Value> {
    let url = format!("https://api.jikan.moe/v4/anime/{}", id);
    let res = reqwest::get(&url)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    if res.status().is_success() {
        let body = res.text().await.map_err(|e| anyhow!(e.to_string()))?;
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;
        Ok(json)
    } else {
        Err(anyhow!(
            "Failed to fetch anime details. Status: {}",
            res.status()
        ))
    }
}

async fn jikan_fetch_related_sequel(id: i64) -> Result<i32> {
    let url = format!("https://api.jikan.moe/v4/anime/{}/relations", id);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: Value = serde_json::from_str(&body).expect("Failed to parse JSON");
        //println!("Sequel data: {:?}", json.clone());
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

// I made this, but then realized that theres no point merging it like this, since we need the name that we delete by merging x)
// So uhh, this is still fucked, but its too late rn for me to give a fuck about fixing it.
fn merge_seasons(series: Vec<Season>) -> Vec<Season> {
    let mut merged_series: Vec<Season> = Vec::new();
    let mut season_map: std::collections::HashMap<String, Season> =
        std::collections::HashMap::new();

    for season in series {
        if let Some(part_index) = season.title.find("Part") {
            // Extract the base title (everything before "Part")
            let base_title = season.title[..part_index].trim().to_string();

            // Merge the part with the base title in the map
            if let Some(existing_season) = season_map.get_mut(&base_title) {
                existing_season.episodes += season.episodes;
            } else {
                season_map.insert(base_title, season);
            }
        } else {
            season_map.insert(season.title.clone(), season);
        }
    }

    merged_series.extend(season_map.into_values());

    let mut current_season_num = 1;
    for season in merged_series.iter_mut() {
        if season.season > current_season_num {
            current_season_num += 1;
            season.season = current_season_num;
        } else if season.season == current_season_num {
            current_season_num += 1;
        }
    }

    merged_series
}
