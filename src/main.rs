use anyhow::Result;
use chrono::Local;
use log::{debug, error, info, trace, warn};
use solana_sdk::signature;
use tokio::time::{Duration, sleep};

mod requests;
use requests::*;

mod database;
use database::*;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logger()?;
    let database = Database::new_pool().await?;
    let helius_api = HeliusApi::new();

    let mut cur_last_signature: Option<String> = None;
    let mut sum: usize = 0;
    let adress = "CKALuhodLSb7F7YtTKNYcBUtFReHL2biMMihSJhMKZm6";

    loop {
        // Сбор всех подписей
        let (responce, last_signature) = helius_api
            .get_signatures(adress, cur_last_signature)
            .await?;

        let res_len = responce.result.len();
        sum += res_len;

        info!("Полученно {res_len} подписей, всего {sum}");

        database
            .write_signatures(&responce, adress)
            .await
            .inspect(|_| info!("Сохраненно в базу данных"))?;

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
                Local::now().naive_local().format("%H:%M:%S%.9f"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stderr())
        .chain(fern::log_file("output.log")?)
        .apply()?;

    Ok(())
}
