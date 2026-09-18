use rusqlite::Connection;
use serde::Serialize;
use std::{fs,path::PathBuf};
use tauri::{AppHandle,Manager};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Dashboard{received_today:i64,awaiting_print:i64}

fn db(app:&AppHandle)->Result<Connection,String>{
 let dir:PathBuf=app.path().app_data_dir().map_err(|e|e.to_string())?;
 fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
 let conn=Connection::open(dir.join("tailor.sqlite")).map_err(|e|e.to_string())?;
 conn.execute_batch("CREATE TABLE IF NOT EXISTS orders(id INTEGER PRIMARY KEY,received_date TEXT NOT NULL,quantity INTEGER NOT NULL DEFAULT 1,print_status TEXT NOT NULL DEFAULT 'بانتظار الطباعة');").map_err(|e|e.to_string())?;
 Ok(conn)
}
#[tauri::command]
fn dashboard_summary(app:AppHandle)->Result<Dashboard,String>{
 let conn=db(&app)?;
 let received_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE received_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let awaiting_print=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE print_status='بانتظار الطباعة'",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(Dashboard{received_today,awaiting_print})
}
fn main(){tauri::Builder::default().invoke_handler(tauri::generate_handler![dashboard_summary]).run(tauri::generate_context!()).expect("تعذر تشغيل TAILOR");}