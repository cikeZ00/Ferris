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

    let result_path = errai("Dr. Stone", "3x10", "fr").await.unwrap();

    if result_path == "" {
        println!("Failed to download the file.");
        std::process::exit(0);
    }

    println!("Downloaded to: {}", result_path);

    // Then we handle uploading the file to bazarr here
    upload("1", "46", "fr", false, false, result_path.clone()).await;

    //Then we delete the file after we're done with it
    //fs::remove_file(result_path).expect("Failed to delete file.");
}
