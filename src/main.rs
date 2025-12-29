use anyhow::Result;
use std::{fs::OpenOptions, io::Write};

mod requests;
use requests::*;

#[tokio::main]
async fn main() -> Result<()> {
    let helius_api = HeliusApi::new();

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .write(true)
        .open("respnse.txt")?;

    let mut cur_last_signature: Option<String> = None;

    loop {
        let (responce, last_signature) = helius_api
        .get_signatures("2ZCQ18QjibZZCPfcCesdZ1y2WMmZKd5rKZLyc2sjYGir", cur_last_signature).await?;

        let response_text = format!("{:#?}", responce.result);
        file.write_all(response_text.as_bytes())?;

        if responce.result.len() < 1000 {
            break;
        }

        cur_last_signature = Some(last_signature);
    }

    Ok(())
}
