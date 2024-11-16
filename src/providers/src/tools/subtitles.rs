use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use std::io::Cursor;

// TODO: Complete language short codes, configurable search patterns for specific series
pub async fn fetch_sub_url(client: &Client, url: &str, headers: &HeaderMap) -> Result<String> {
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

pub async fn fetch_subtitle(
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
        let base_url = "https://www.erai-raws.info/subs/";

        if let Some(dirlist) = document.find(Attr("id", "directory-listing")).next() {
            for row in dirlist.find(Name("li")) {
                let link = format!(
                    "{}{}",
                    base_url,
                    row.attr("data-href").unwrap_or("#").to_string()
                );
                let name = row.attr("data-name").unwrap_or("#").to_string();

                if name != ".." && !link.ends_with(".7z") {
                    if name.eq_ignore_ascii_case(language) {
                        println!("Directory is a full language name.");
                        return fetch_subtitle_from_language_dir(
                            client, &link, headers, episode, language,
                        )
                        .await;
                    }

                    if let Some(caps) = Regex::new(r"01 ~ (\d+)").unwrap().captures(&name) {
                        let max_episode = caps[1].parse::<u32>().unwrap_or(0);
                        println!("Directory is a range of episodes.");
                        if episode <= max_episode {
                            return fetch_subtitle_from_range_dir(
                                client, &link, headers, episode, language,
                            )
                            .await;
                        }
                    }

                    if Regex::new(r"^\d+").unwrap().is_match(&name) {
                        println!("Directory is a list of episodes.");

                        if name.contains(&format!("{:02}", episode)) {
                            return fetch_subtitle_from_episode_dirs(
                                client, &link, headers, episode, language,
                            )
                            .await;
                        }
                    }
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

pub async fn fetch_subtitle_from_language_dir(
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

        let language_full = match language.to_lowercase().as_str() {
            "en" => "English",
            "jp" => "Japanese",
            "es" => "Spanish",
            "fr" => "French",
            "de" => "German",
            "it" => "Italian",
            "pt" => "Portuguese",
            "ru" => "Russian",
            "zh" => "Chinese",
            _ => language,
        };

        if let Some(dirlist) = document.find(Attr("id", "directory-listing")).next() {
            for row in dirlist.find(Name("li")) {
                let link = row.attr("data-href").unwrap_or("#").to_string();
                let name = row.attr("data-name").unwrap_or("#").to_string();

                if name != ".." && !link.ends_with(".7z") {
                    println!("Name: {}", name);
                    if name.contains(&format!("{:02}", episode)) || name.contains(language_full) {
                        println!("Found: {}", link);
                        return Ok(link);
                    }
                }
            }
        }
    }

    Ok(String::new())
}

pub async fn fetch_subtitle_from_range_dir(
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

        let language_full = match language.to_lowercase().as_str() {
            "en" => "English",
            "jp" => "Japanese",
            "es" => "Spanish",
            "fr" => "French",
            "de" => "German",
            "it" => "Italian",
            "pt" => "Portuguese",
            "ru" => "Russian",
            "zh" => "Chinese",
            _ => language,
        };

        if let Some(dirlist) = document.find(Attr("id", "directory-listing")).next() {
            for row in dirlist.find(Name("li")) {
                let link = row.attr("data-href").unwrap_or("#").to_string();
                let name = row.attr("data-name").unwrap_or("#").to_string();

                if name != ".." && !link.ends_with(".7z") {
                    if name.contains(&format!("{:02}", episode)) || name.contains(&language_full) {
                        return fetch_subtitle_from_language_dir(
                            client,
                            &format!("https://www.erai-raws.info/subs/{}", link),
                            headers,
                            episode,
                            language,
                        )
                        .await;
                    }
                }
            }
        }
    }

    Ok(String::new())
}

pub async fn fetch_subtitle_from_episode_dirs(
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
                    let language_full = match language.to_lowercase().as_str() {
                        "en" => "English",
                        "jp" => "Japanese",
                        "es" => "Spanish",
                        "fr" => "French",
                        "de" => "German",
                        "it" => "Italian",
                        "pt" => "Portuguese",
                        "ru" => "Russian",
                        "zh" => "Chinese",
                        _ => language,
                    };

                    if name.eq_ignore_ascii_case(language)
                        || name.eq_ignore_ascii_case(language_full)
                    {
                        println!("Redirecting to language directory.");
                        return fetch_subtitle_from_language_dir(
                            client,
                            &format!("https://www.erai-raws.info/subs/{}", link),
                            headers,
                            episode,
                            language,
                        )
                        .await;
                    }

                    if name.contains(language) {
                        println!("Found: {}", name);
                        return Ok(link);
                    }

                    if name.starts_with(&format!("{:02}", episode)) {
                        println!("Name: {} | Link: {}", name, link);
                        return Ok(link);
                    }
                }
            }
        }
    }

    Ok(String::new())
}

pub async fn save_sub_from_url(
    client: &Client,
    headers: &HeaderMap,
    url: &str,
    name: &str,
) -> Result<String> {
    let full_url = format!("https://www.erai-raws.info/subs/{}", url);

    if !std::path::Path::new("temp").exists() {
        std::fs::create_dir("temp").unwrap();
    }
    let mut file = std::fs::File::create(format!("temp/{}", name)).unwrap();

    let mut download_headers = headers.clone();
    download_headers.insert(
        "User-Agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
        ),
    );
    download_headers.insert(
        "Accept",
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    download_headers.insert(
        "Accept-Language",
        HeaderValue::from_static("en-US,en;q=0.5"),
    );
    download_headers.insert(
        "Accept-Encoding",
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    download_headers.insert("DNT", HeaderValue::from_static("1"));
    download_headers.insert("Sec-GPC", HeaderValue::from_static("1"));
    download_headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    download_headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));
    download_headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));
    download_headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));
    download_headers.insert("Sec-Fetch-Site", HeaderValue::from_static("same-origin"));
    download_headers.insert("Sec-Fetch-User", HeaderValue::from_static("?1"));
    download_headers.insert("Priority", HeaderValue::from_static("u=0, i"));
    download_headers.insert("TE", HeaderValue::from_static("trailers"));

    let res = client
        .get(&full_url)
        .headers(download_headers)
        .send()
        .await
        .unwrap();

    if res.status().is_success() {
        let mut content = Cursor::new(res.bytes().await.unwrap());
        println!("Downloaded subtitle: {}", name);
        std::io::copy(&mut content, &mut file).unwrap();
        return Ok(format!("temp/{}", name));
    } else {
        println!("Failed to download subtitle. Status: {}", res.status());
        return Ok("".to_string());
    }
}
