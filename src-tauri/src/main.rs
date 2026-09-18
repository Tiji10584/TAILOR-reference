use rusqlite::Connection;
use serde::Serialize;
use std::{fs,path::PathBuf};
use tauri::{AppHandle,Manager};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Dashboard{received_today:i64,tailored_today:i64,due_today:i64}

fn has_column(conn:&Connection,column:&str)->Result<bool,String>{
 let mut statement=conn.prepare("PRAGMA table_info(orders)").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|row.get::<_,String>(1)).map_err(|e|e.to_string())?;
 for row in rows{
  if row.map_err(|e|e.to_string())?==column{return Ok(true)}
 }
 Ok(false)
}

fn db(app:&AppHandle)->Result<Connection,String>{
 let dir:PathBuf=app.path().app_data_dir().map_err(|e|e.to_string())?;
 fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
 let conn=Connection::open(dir.join("tailor.sqlite")).map_err(|e|e.to_string())?;
 conn.execute_batch("CREATE TABLE IF NOT EXISTS orders(id INTEGER PRIMARY KEY,received_date TEXT NOT NULL,quantity INTEGER NOT NULL DEFAULT 1,print_status TEXT NOT NULL DEFAULT 'بانتظار الطباعة');").map_err(|e|e.to_string())?;
 if !has_column(&conn,"tailored_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN tailored_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"delivery_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN delivery_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 Ok(conn)
}

#[tauri::command]
fn dashboard_summary(app:AppHandle)->Result<Dashboard,String>{
 let conn=db(&app)?;
 let received_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE received_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let tailored_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE tailored_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let due_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE delivery_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(Dashboard{received_today,tailored_today,due_today})
}

fn main(){tauri::Builder::default().invoke_handler(tauri::generate_handler![dashboard_summary]).run(tauri::generate_context!()).expect("تعذر تشغيل TAILOR");}