//! 《StackCity RT（像素疊界）》核心程式庫。
//!
//! 整合純 Rust Sprite-Stacking 體素造物引擎、SQLite 單檔自包含世界資料庫，
//! 以及 Axum 網頁與資源服務介面。

pub mod api;
pub mod db;
pub mod voxel;
