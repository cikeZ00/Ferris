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
        .header(USER_AGENT, "Ferris")
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
// We want to return the series type, episode_number, seriesTitle, sonarrSeriesId, SonarrEpisodeId, as well as the list of missing_subtitles
//
// {
// "data": [
//   {
//     "seriesTitle": "Dr. Stone",
//     "episode_number": "1x13",
//     "episodeTitle": "Masked Warrior",
//     "missing_subtitles": [
//       {
//         "name": "Croatian",
//         "code2": "hr",
//         "code3": "hrv",
//         "forced": false,
//         "hi": false
//       }
//     ],
//     "sonarrSeriesId": 1,
//     "sonarrEpisodeId": 13,
//     "sceneName": null,
//     "tags": [
//       "anime"
//     ],
//     "seriesType": "anime"
//   },
pub async fn fetch_wanted_shows(
) -> Result<Vec<HashMap<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    let (base_url, token) = get_url();
    let url = format!("{}/api/episodes/wanted?start=0&length=-1", base_url);
    let client = Client::new();

    let response = client
        .get(&url)
        .header(USER_AGENT, "Ferris")
        .header("X-API-KEY", token)
        .send()
        .await?;

    if response.status().is_success() {
        let json: serde_json::Value = response.json().await?;
        let data = json["data"]
            .as_array()
            .ok_or("Expected data to be an array")?;

        let mut result = Vec::new();
        for item in data {
            let mut show_info = HashMap::new();
            show_info.insert("seriesTitle".to_string(), item["seriesTitle"].clone());
            show_info.insert("episode_number".to_string(), item["episode_number"].clone());
            show_info.insert("seriesType".to_string(), item["seriesType"].clone());
            show_info.insert("sonarrSeriesId".to_string(), item["sonarrSeriesId"].clone());
            show_info.insert(
                "sonarrEpisodeId".to_string(),
                item["sonarrEpisodeId"].clone(),
            );
            show_info.insert(
                "missing_subtitles".to_string(),
                item["missing_subtitles"].clone(),
            );
            result.push(show_info);
        }

        Ok(result)
    } else {
        Err(format!("Failed to fetch wanted shows: {:?}", response.text().await?).into())
    }
}
