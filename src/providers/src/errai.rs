use regex::Regex;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, COOKIE, REFERER, USER_AGENT,
};
use reqwest::{Client, Result};
use select::document::Document;
use select::predicate::{Attr, Name};
use std::fs;
use strsim::{jaro_winkler, levenshtein};

pub async fn errai(name: &str, es: &str, language: &str) -> Result<()> {
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

    println!("Searching for: {}", search_term);

    // Generate titles based on the provided episode string (e.g., "2x18"), focusing only on the season
    let season = extract_season(es);
    let titles = generate_titles(&search_term, season);

    let sample_list = vec![
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka: Familia Myth V",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka: Familia Myth IV Part 2",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka: Familia Myth IV",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka: Orion no Ya",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka III",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka II",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka Gaiden: Sword Oratoria",
        "Dungeon ni Deai wo Motomeru no wa Machigatteiru Darou ka",
    ]; // Example list of strings to compare against

    // Compare the search term with the sample list and return the most similar one
    for sample in sample_list {
        for title in &titles {
            let compared = compare_titles(title, sample);
            println!("{} | {} | {}", title, sample, compared);
        }
    }

    // We hijack the main website's search functionality to get the search results
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

        titles.push(format!("{} {} ", base_title, season));
        titles.push(format!("{} Season {} ", base_title, season));
        titles.push(format!("{} S{} ", base_title, season));
        titles.push(format!("{} {}{} Season ", base_title, season, ordinal));
        titles.push(format!("{} {} ", base_title, roman_season));
        titles.push(format!("{} Season {} ", base_title, roman_season));
    }

    titles
}

// Compare every generated title with every sample title and return the most similar one for each generated title
fn compare_titles(generated_title: &str, sample_title: &str) -> f32 {
    // Calculate Levenshtein distance between the two titles
    let lev_distance = levenshtein(generated_title, sample_title);

    // Determine the maximum possible distance based on the length of the longer title
    let max_len = generated_title.len().max(sample_title.len());

    // If max_len is 0 (both titles are empty), return perfect match score
    if max_len == 0 {
        return 1.0;
    }

    // Calculate base similarity score
    let similarity_score = 1.0 - (lev_distance as f32 / max_len as f32);

    // Extract season number from the sample title
    let season_number = extract_season_number(sample_title);

    // Print the extracted season number for debugging
    println!("Season number: {}", season_number);

    // Create a string representation of the season number
    let season_str = season_number.to_string();
    let roman_numeral = to_roman_numeral(season_number);

    // Check for various formats in the generated title
    let has_correct_number = generated_title.contains(&season_str)
        || generated_title.contains(&roman_numeral)
        || generated_title.contains(&format!("Season {}", season_str))
        || generated_title.contains(&format!("{}th Season", season_str))
        || generated_title.contains(&format!("S{}", season_str))
        || generated_title.contains(&format!("{} Season", season_str));

    // Apply a small bonus for matching the correct season number or Roman numeral
    let bonus = if has_correct_number { 0.2 } else { 0.0 }; // Adjust the bonus weight as needed

    // Penalty for missing the season number or Roman numeral
    let missing_number_penalty = if !has_correct_number { 0.5 } else { 0.0 }; // Adjust the penalty weight as needed

    // Penalty for missing all characters in the generated title
    let all_characters_present = sample_title
        .split_whitespace()
        .all(|token| generated_title.contains(token));
    let missing_characters_penalty = if !all_characters_present { 0.3 } else { 0.0 }; // Adjust the penalty weight as needed

    // Introduce a penalty for mismatches based on the length of the titles
    let penalty = (lev_distance as f32 / max_len as f32) * 0.4; // Penalty weight can be adjusted

    // Final score with bonus and penalties
    let final_score =
        (similarity_score + bonus - penalty - missing_characters_penalty - missing_number_penalty)
            .clamp(0.0, 1.0); // Ensure score is within [0.0, 1.0]

    final_score
}

fn extract_season_number(title: &str) -> usize {
    // Regex to match "Season X", "S X", "Xth Season", or just "X"
    let re = Regex::new(r"(?i)(?:season\s+|s|)(\d+)(?:th)?\s*season?|(\d+)").unwrap();

    if let Some(caps) = re.captures(title) {
        // Try to parse the first capturing group as a usize
        if let Some(season_str) = caps.get(1) {
            return season_str.as_str().parse::<usize>().unwrap_or(0);
        }
        // Check the second capturing group if the first doesn't yield a result
        if let Some(season_str) = caps.get(2) {
            return season_str.as_str().parse::<usize>().unwrap_or(0);
        }
    }
    // Return 0 if no season number found
    0
}

fn to_roman_numeral(num: usize) -> String {
    match num {
        1 => "I".to_string(),
        2 => "II".to_string(),
        3 => "III".to_string(),
        4 => "IV".to_string(),
        5 => "V".to_string(),
        // Add more as needed
        _ => "".to_string(),
    }
}
