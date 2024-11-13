use reqwest::header::USER_AGENT;
use reqwest::multipart::Form;
use reqwest::Client;
use std::fs;

fn get_url() -> (String, String) {
    // If config doesnt exist, create it
    if !fs::metadata("data/bazarr.ini").is_ok() {
        fs::write(
            "data/bazarr.ini",
            "url = Paste Your Bazarr URL Here \n token = Paste Your Bazarr Token Here",
        )
        .expect("Failed to write bazarr.ini file.");
        println!(
            "Please paste your Bazarr URL & Token into data/bazarr.ini and run the program again."
        );
        std::process::exit(0);
    }

    let config = fs::read_to_string("data/bazarr.ini").expect("Failed to read bazarr.ini file.");
    let url = config.split("=").collect::<Vec<&str>>()[1].trim();
    let token = config.split("=").collect::<Vec<&str>>()[3].trim();
    (url.to_string(), token.to_string())
}

//POST /episodes/subtitles Parameters: (seriesid(int), episodeid(int), language(string), forced(bool), hi(bool), file(file))
// /api/episodes/subtitles?seriesid=10&episodeid=32&language=En&forced=false&hi=false
// Example curl command:
// curl -X 'POST' \
// 'http://192.168.0.13:6767/api/episodes/subtitles?seriesid=10&episodeid=32&language=En&forced=false&hi=false' \
// -H 'accept: application/json' \
// -H 'X-API-KEY: 95e9d038d5fc7e7d257db2e7d1763cd6' \
// -H 'Content-Type: multipart/form-data' \
// -F 'file=@Fate Zero - S01E01 - Summoning Ancient Heroes - VOSTFR 1080p 10bit HDLight BluRay AAC 2.0 .ass;type=text/x-ssa'
pub async fn upload(
    series: &str,
    episode: &str,
    language: &str,
    forced: bool,
    hi: bool,
    file: String,
) {
    println!("Uploading to Bazarr...");
    let url = format!("{}/epicsodes/upload", get_url().0);
    let client = Client::new();
    let form = Form::new()
        .text("seriesid", series.to_string())
        .text("episodeid", episode.to_string())
        .text("language", language.to_string())
        .text("forced", forced.to_string())
        .text("hi", hi.to_string())
        .file("file", file)
        .await
        .expect("Failed to create form");

    let response = client
        .post(&url)
        .header(USER_AGENT, "reqwest")
        .header("X-API-KEY", get_url().1)
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
pub async fn fetch_wanted_shows() {
    println!("Fetching wanted from Bazarr...");
    let url = format!("{}/api/episodes/wanted?start=0&length=-1", get_url().0);
}
