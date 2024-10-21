use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use std::fs;
use strsim::levenshtein;

pub async fn errai(name: &str, es: &str, _language: &str) -> Result<()> {
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
    let search_term = jikan_resolve_title(name).await?;

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

    // Generate titles based on the provided episode string (e.g., "2x18"), focusing only on the season
    let season = extract_season(es);
    let guess_titles = generate_titles(&search_term, season);


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

        let find_result = find_best_match(series_titles_list.iter().map(|s| s.as_str()).collect(), guess_titles.iter().map(|s| s.as_str()).collect()).unwrap();
        println!("Best Match: {}", find_result);

        // Fetch the subtitle dir page of best match from title_pages
        let best_match_url = title_pages.iter().find(|(title, _)| title == find_result);

        if let Some((_, link)) = best_match_url {
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

// Function to extract the season from the input format (e.g., "2x18")
fn extract_season(input: &str) -> Option<u32> {
    let re = Regex::new(r"(\d+)x\d+").unwrap();
    if let Some(caps) = re.captures(input) {
        return caps[1].parse::<u32>().ok();
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

fn int_to_roman(num: u32) -> String {
    let mut result = String::new();
    let roman_numerals = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut n = num;

    for &(value, symbol) in &roman_numerals {
        while n >= value {
            result.push_str(symbol);
            n -= value;
        }
    }

    result
}

fn ordinal_suffix(num: u32) -> String {
    match num % 10 {
        1 if num % 100 != 11 => "st",
        2 if num % 100 != 12 => "nd",
        3 if num % 100 != 13 => "rd",
        _ => "th",
    }
    .to_string()
}

fn generate_titles(base_title: &str, season: Option<u32>) -> Vec<String> {
    let mut titles = Vec::new();

    if let Some(season) = season {
        let roman_season = int_to_roman(season); // Convert season to Roman numeral
        let ordinal = ordinal_suffix(season); // Get the ordinal suffix

        if season > 1 {
            titles.push(format!("{} {}", base_title, season));
            titles.push(format!("{} Season {}", base_title, season));
            titles.push(format!("{} S{}", base_title, season));
            titles.push(format!("{} {}{} Season", base_title, season, ordinal));
            titles.push(format!("{} {}", base_title, roman_season));
            titles.push(format!("{} Season {}", base_title, roman_season));
        } else {
            // Edge case for season 1
            titles.push(base_title.to_string());
        }
    }

    titles
}

fn compare_titles(generated_title: &str, sample_title: &str) -> f32 {
    let lev_distance = levenshtein(generated_title, sample_title);
    let max_len = generated_title.len().max(sample_title.len());

    if max_len == 0 {
        return 1.0; // Perfect match if both are empty
    }

    let similarity_score = 1.0 - (lev_distance as f32 / max_len as f32);

    fn is_roman_numeral(s: &str) -> bool {
        matches!(
            s.to_uppercase().as_str(),
            "I" | "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X"
        )
    }

    // Extract possible season number from sample title (either digits or Roman numerals)
    let season_number = sample_title
        .split_whitespace()
        .find(|token| token.chars().all(|c| c.is_digit(10)) || is_roman_numeral(token));

    let has_correct_number = if let Some(season_str) = season_number {
        [
            season_str,
            &format!("Season {}", season_str),
            &format!("{}th Season", season_str),
            &format!("S{}", season_str),
            &format!("{} Season", season_str),
        ]
        .iter()
        .any(|format| generated_title.contains(format))
    } else {
        false
    };

    // Apply bonus for season number match
    let bonus = if has_correct_number { 0.2 } else { 0.0 };

    // Penalties for missing season number or characters
    let missing_number_penalty = if !has_correct_number { 0.5 } else { 0.0 };
    let all_characters_present = sample_title
        .split_whitespace()
        .all(|word| generated_title.contains(word));
    let missing_characters_penalty = if !all_characters_present { 0.3 } else { 0.0 };

    // Final score with bonuses and penalties
    (similarity_score + bonus - missing_number_penalty - missing_characters_penalty).clamp(0.0, 1.0)
}

fn find_best_match<'a>(titles: Vec<&'a str>, title_list: Vec<&'a str>) -> Option<&'a str> {
    // Compare the search term with the sample list and return the most similar one
    let mut title_score = 0.0;
    let mut best_title = "";

    // Fetch the subtitle with the highest score
    for sample in title_list {
        for title in &titles {
            let compared = compare_titles(title, sample);
            if compared > title_score {
                title_score = compared;
                best_title = title;
            }
        }
    }

    println!("Title: {} | Score: {}", best_title, title_score);
    Some(best_title)
}