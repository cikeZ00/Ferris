use reqwest::header::USER_AGENT;
use reqwest::multipart::Form;
use reqwest::Client;
use std::collections::HashMap;
use std::fs;

fn get_url() -> (String, String) {
    // If config doesn't exist, create it
    if !fs::metadata("data/bazarr.ini").is_ok() {
        fs::write(
            "data/bazarr.ini",
            "url = Paste Your Bazarr URL Here\n token = Paste Your Bazarr Token Here",
        )
        .expect("Failed to write bazarr.ini file.");
        println!(
            "Please paste your Bazarr URL & Token into data/bazarr.ini and run the program again."
        );
        std::process::exit(0);
    }

    let config = fs::read_to_string("data/bazarr.ini").expect("Failed to read bazarr.ini file.");
    let mut config_map: HashMap<&str, &str> = HashMap::new();

    for line in config.lines() {
        let parts: Vec<&str> = line.split('=').map(|s| s.trim()).collect();
        if parts.len() == 2 {
            config_map.insert(parts[0], parts[1]);
        }
    }

    let url = config_map
        .get("url")
        .expect("URL not found in config")
        .to_string();
    let token = config_map
        .get("token")
        .expect("Token not found in config")
        .to_string();

    (url, token)
}

pub async fn upload(
    series: &str,
    episode: &str,
    language: &str,
    forced: bool,
    hi: bool,
    file: String,
) {
    println!("Uploading to Bazarr...");
    let (base_url, token) = get_url();
    let url = format!(
        "{}/api/episodes/subtitles?seriesid={}&episodeid={}&language={}&forced={}&hi={}",
        base_url, series, episode, language, forced, hi
    );
    let client = Client::new();
    let form = Form::new()
        .file("file", file)
        .await
        .expect("Failed to create form");

    let response = client
        .post(&url)
        .header(USER_AGENT, "reqwest")
        .header("X-API-KEY", token)
        .multipart(form)
        .send()
        .await
        .expect("Failed to upload file");

    if response.status().is_success() {
        println!("Upload successful!");
    } else {
        println!("Upload failed: {:?}", response.text().await);
    }
}

// /api/episodes/wanted?start=0&length=-1
// We also need the episodeid and seriesid
pub async fn fetch_wanted_shows() {
    println!("Fetching wanted from Bazarr...");
    let _url = format!("{}/api/episodes/wanted?start=0&length=-1", get_url().0);
}
