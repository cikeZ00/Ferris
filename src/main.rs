use providers::errai::errai;
use std::fs;

#[tokio::main]
async fn main() {
    // Create a new data directory if it doesnt exist and write a config.ini file
    fs::create_dir_all("data").expect("Failed to create data directory.");
    if !fs::metadata("data/config.ini").is_ok() {
        fs::write("data/config.ini", "errai_cookie = Paste Your Cookie Here")
            .expect("Failed to write config.ini file.");

        println!("Please paste your cookie into data/config.ini and run the program again.");
        std::process::exit(0);
    }

    errai("Re: ZERO, Starting Life in Another World", 2, 1, "French")
        .await
        .unwrap();

    println!("Hello, world!");
}
