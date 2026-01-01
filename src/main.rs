use anyhow::Result;
use std::{fs::OpenOptions, io::Write};
use tokio::time::{sleep, Duration};
use chrono::Local;
use log::{debug, error, info, trace, warn};

mod requests;
use requests::*;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger()?;
    let helius_api = HeliusApi::new();

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .write(true)
        .open("respnse.txt")?;

    let mut cur_last_signature: Option<String> = None;

    let mut sum: usize = 0;

    loop {
        let (responce, last_signature) = helius_api
        .get_signatures("H3B7dM826FyyZe2ehuu6zzFEFvL1HdLvk994pzpfakJp", cur_last_signature).await?;

        let response_text = format!("{:#?}", responce.result);
        file.write_all(response_text.as_bytes())?;

        let res_len = responce.result.len();
        sum += res_len;

        info!("Полученно {res_len} подписей, всего {sum}");

        if res_len < 1000 {
            break;
        }

        cur_last_signature = Some(last_signature);

        sleep(Duration::from_millis(125)).await;
    }

    Ok(())
}

fn setup_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "| {} | {} | {} | {}",
                Local::now().naive_local(),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(std::io::stderr())
        .chain(fern::log_file("output.log")?)
        .apply()?;

    Ok(())
}
