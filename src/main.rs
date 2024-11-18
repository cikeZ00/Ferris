use bazarr::bazarr::fetch_wanted_shows;
use bazarr::bazarr::upload;
use providers::errai::errai;
use std::fs;

#[tokio::main]
async fn main() {
    // Create a new datbazarra directory if it doesnt exist and write a config.ini file
    fs::create_dir_all("data").expect("Failed to create data directory.");
    if !fs::metadata("data/config.ini").is_ok() {
        fs::write("data/config.ini", "errai_cookie = Paste Your Cookie Here")
            .expect("Failed to write config.ini file.");
        println!("Please paste your cookie into data/config.ini and run the program again.");
        std::process::exit(0);
    }

    let wanted = fetch_wanted_shows().await;
    match wanted {
        Ok(shows) => {
            if !shows.is_empty() {
                for show in shows {
                    if show["seriesTitle"] == "Dr. Stone" {
                        println!("Show name: {}", show["seriesTitle"]);
                        let wanted_ep_sonarr = show["sonarrEpisodeId"].to_string();
                        let wanted_series_sonarr = show["sonarrSeriesId"].to_string();
                        let wanted_series = show["seriesTitle"].to_string();
                        let wanted_ep = show["episode_number"].to_string();
                        for missing_sub in show["missing_subtitles"].as_array().unwrap() {
                            let missing_lang = missing_sub["code2"].to_string();
                            println!("Missing language: {}", missing_lang);
                            // skip croatian
                            if missing_lang.contains("hr") || missing_lang == "hrv" {
                                println!("Skipping Croatian");
                            } else {
                                let result_path = errai(&wanted_series, &wanted_ep, &missing_lang)
                                    .await
                                    .unwrap();

                                if result_path != "" {
                                    println!("Downloaded to: {}", result_path);

                                    // Then we handle uploading the file to bazarr here
                                    upload(
                                        &wanted_series_sonarr,
                                        &wanted_ep_sonarr,
                                        &missing_lang,
                                        false,
                                        false,
                                        result_path.clone(),
                                    )
                                    .await;

                                    //Then we delete the file after we're done with it
                                    fs::remove_file(result_path).expect("Failed to delete file.");
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to fetch wanted shows: {}", e);
            std::process::exit(1);
        }
    }
}
