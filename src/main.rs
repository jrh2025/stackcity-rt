//! 《StackCity RT（像素疊界）》伺服器執行入口。

use clap::Parser;
use stackcity_rt::api::{build_router, AppState};
use stackcity_rt::db::ensure_data_dir_initialized;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;

/// 命令列啟動參數
#[derive(Parser, Debug)]
#[command(name = "stackcity-rt", about = "《StackCity RT（像素疊界）》微縮世界沙盒伺服器")]
struct Args {
    /// 綁定主機位址
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// 預設伺服器連接埠（若已佔用將自動順延）
    #[arg(long, default_value_t = 8788)]
    port: u16,
    /// 啟動後不自動開啟瀏覽器
    #[arg(long, default_value_t = false)]
    no_browser: bool,
    /// 世界資料庫與資產庫儲存目錄
    #[arg(long, default_value = "./worlds")]
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    println!("正在初始化《StackCity RT（像素疊界）》SQLite 世界目錄：{:?}", args.data_dir);
    ensure_data_dir_initialized(&args.data_dir)?;

    let mut chosen_listener = None;
    let mut chosen_port = args.port;
    for offset in 0..50u16 {
        let p = args.port.saturating_add(offset);
        let addr_str = format!("{}:{}", args.host, p);
        if let Ok(listener) = TcpListener::bind(&addr_str).await {
            chosen_listener = Some(listener);
            chosen_port = p;
            break;
        }
    }

    let listener = chosen_listener.ok_or("找不到可用的網路連接埠")?;
    let url = format!("http://{}:{}", args.host, chosen_port);
    println!("============================================================");
    println!("  StackCity RT（像素疊界）伺服器已就緒！");
    println!("  本機互動網址：{url}");
    println!("  世界檔案路徑：{:?}", args.data_dir);
    println!("============================================================");

    if !args.no_browser {
        let open_url = url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(450));
            let _ = open::that(open_url);
        });
    }

    let app = build_router(AppState {
        data_dir: args.data_dir,
    });
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
