use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::Name;
use std::fs;

// Import the necessary modules from the tools directory
use crate::tools::jikan::jikan_fetch_anime;
use crate::tools::subtitles::{fetch_sub_url, fetch_subtitle, save_sub_from_url};

pub async fn errai(name: &str, es: &str, language: &str) -> Result<String> {
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
        return Ok("".to_string());
    }

    let client = Client::new();
    let anime = jikan_fetch_anime(name, es).await?;

    if anime.0 == "None" {
        println!("Anime not found.");
        return Ok("".to_string());
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

        // We need to look out for parts in titles, if we want part 1 but the title doesnt contain part one
        // it might accidentally fetch part 2 instead, some titles also have english names appended to them
        let link = title_pages
            .iter()
            .find(|(title, _)| title.to_lowercase() == anime.0.to_lowercase());

        if let Some((_, link)) = link {
            let sub_dir = match fetch_sub_url(&client, link, &headers).await {
                Ok(url) => url,
                Err(e) => {
                    println!("Failed to fetch article details: {}", e);
                    return Ok("".to_string());
                }
            };

            let subtitle =
                match fetch_subtitle(&client, &sub_dir, &headers, anime.1, &language).await {
                    Ok(sub) => sub,
                    Err(e) => {
                        println!("Failed to fetch subtitles: {}", e);
                        return Ok("".to_string());
                    }
                };

            let sub_extension = subtitle.split('.').last().unwrap_or("ass");
            return save_sub_from_url(
                &client,
                &headers,
                &subtitle,
                &format!("subtitle.{}", sub_extension),
            )
            .await;
        }
    } else {
        println!("Failed to fetch search results. Status: {}", res.status());
    }

    Ok("".to_string())
}
