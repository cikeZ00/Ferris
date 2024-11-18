use crate::tools::cache::Cache;
use anyhow::anyhow;
use regex::Regex;
use reqwest::Result;
use serde_json::Value;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct Season {
    title: String,
    season: i32,
    part: i32,
    episodes: i32,
}

pub async fn jikan_fetch_anime(title: &str, es: &str) -> Result<(String, u32)> {
    let cache = Cache::new(Duration::from_secs(3600)); // Cache TTL of 1 hour

    let url = format!("https://api.jikan.moe/v4/anime?q={}", title);
    let res = reqwest::get(&url).await?;

    if res.status().is_success() {
        let body = res.text().await?;
        let json: Value = serde_json::from_str(&body).expect("Failed to parse JSON");

        let mut id = json["data"][0]["mal_id"].clone();
        println!("Fetched ID: {}", id);

        for result in json["data"].as_array().unwrap() {
            if result["type"].to_string() == "\"TV\"" || result["type"].to_string() == "\"ONA\"" {
                id = result["mal_id"].clone();
                break;
            }
        }

        if let Some((wanted_season, wanted_episode)) = extract_season_episode(es) {
            println!(
                "Wanted Season: {} | Wanted Episode {}",
                wanted_season, wanted_episode
            );

            let series = build_full_series(id.as_i64().unwrap() as i32, &cache).await?;

            let merged_series = merge_seasons(series);

            if let Some(season) = merged_series
                .iter()
                .find(|season| season.season == wanted_season as i32)
            {
                println!("Found season: {:?}", season);

                let mut remaining_episodes = wanted_episode;

                if remaining_episodes > season.episodes as u32 {
                    let next_part = merged_series.iter().find(|next_season| {
                        next_season.season == season.season && next_season.part == season.part + 1
                    });

                    if let Some(next_part) = next_part {
                        remaining_episodes -= season.episodes as u32;
                        println!("Fetching next part: {:?}", next_part);
                        println!("Remaining episodes to find: {}", remaining_episodes);

                        if remaining_episodes <= next_part.episodes as u32 {
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
                        // Unsure this is needed
                        let episode_info: (String, u32) = (season.title.clone(), season.episodes as u32);
                        println!("In {}: Episode {}", season.title, season.episodes);
                        return Ok(episode_info);
                    }
                } else {
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

    Ok(("None".to_string(), 0))
}

pub async fn jikan_fetch_airing_data(id: i32, cache: &Cache) -> anyhow::Result<Value> {
    sleep(Duration::from_secs(1)).await;
    let cache_key = format!("airing_{}", id);
    if let Some(cached_value) = cache.get(&cache_key) {
        return Ok(cached_value);
    }

    let url = format!("https://api.jikan.moe/v4/anime/{}/episodes", id);
    let res = reqwest::get(&url)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;

    if res.status().is_success() {
        let body = res.text().await.map_err(|e| anyhow!(e.to_string()))?;
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| anyhow!(e.to_string()))?;

        cache.set(cache_key, json.clone());
        return Ok(json);
    }

    Err(anyhow!(
        "Failed to fetch airing data. Status: {}",
        res.status()
    ))
}

pub async fn jikan_fetch_anime_by_id(id: i32, cache: &Cache) -> anyhow::Result<Value> {
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

pub async fn jikan_fetch_related_sequel(id: i64, cache: &Cache) -> Result<i32> {
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

pub fn extract_season_episode(input: &str) -> Option<(u32, u32)> {
    let re = Regex::new(r"(\d+)x(\d+)").unwrap();
    if let Some(caps) = re.captures(input) {
        let season = caps[1].parse::<u32>().ok()?;
        let episode = caps[2].parse::<u32>().ok()?;
        return Some((season, episode));
    }
    None
}

pub fn merge_seasons(series: Vec<Season>) -> Vec<Season> {
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

    merged_series.sort_by(|a, b| a.season.cmp(&b.season).then_with(|| a.part.cmp(&b.part)));
    merged_series
}

pub async fn build_full_series(id: i32, cache: &Cache) -> Result<Vec<Season>> {
    let mut series_full = Vec::new();
    let mut current_id = id;
    let mut season_number = 1;
    let mut part_number = 1;

    loop {
        let anime_data = jikan_fetch_anime_by_id(current_id, cache).await.unwrap();

        let mut episode_count = anime_data["data"]["episodes"].as_i64().unwrap_or(0) as i32;

        if episode_count == 0 {
            let airing_data = jikan_fetch_airing_data(current_id, cache).await.unwrap();
            episode_count = airing_data["data"]["episodes"].as_i64().unwrap_or(0) as i32;
        }

        let title = anime_data["data"]["title"]
            .as_str()
            .unwrap_or("Unknown Title");

        let season_to_build = Season {
            title: title.to_string(),
            season: season_number,
            part: part_number,
            episodes: episode_count,
        };

        if season_to_build.episodes > 3 {
            series_full.push(season_to_build);
        }

        match jikan_fetch_related_sequel(current_id as i64, cache).await {
            Ok(next_id) if next_id != 0 => {
                let next_anime_data = jikan_fetch_anime_by_id(next_id, cache).await.unwrap();
                let next_title = next_anime_data["data"]["title"]
                    .as_str()
                    .unwrap_or("Unknown Title");

                if next_title.contains("2nd Season") && title.contains("2nd Season") {
                    part_number += 1;
                } else if next_title.contains("3rd Season") && title.contains("3rd Season") {
                    part_number += 1;
                } else {
                    if next_anime_data["data"]["episodes"].as_i64() > Some(3) {
                        season_number += 1;
                        part_number = 1;
                    } else {
                        println!("Skipping season with less than 3 episodes: {}", next_title);
                    }
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
