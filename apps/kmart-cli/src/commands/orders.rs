//! `orders` -- what has been bought.

use cli_kit::emit;

use crate::app::App;
use crate::error::AppResult;
use crate::views::OrderList;

pub async fn run(app: &App, limit: u32) -> AppResult<()> {
    let client = app.client().await?;
    let page = client.orders(limit, None).await?;
    let mut out = app.out();
    Ok(emit(&mut out, &OrderList { page: &page })?)
}
