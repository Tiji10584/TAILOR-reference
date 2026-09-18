use rusqlite::Connection;
use serde::Serialize;
use std::{fs,path::PathBuf,sync::Mutex};
use tauri::{AppHandle,Manager,WindowEvent};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Dashboard{received_today:i64,tailored_today:i64,due_today:i64}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct CurrentSession{started_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SessionEntry{started_at:String,ended_at:String}

struct SessionState(Mutex<Option<i64>>);

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
 conn.execute_batch("
  CREATE TABLE IF NOT EXISTS orders(
   id INTEGER PRIMARY KEY,
   received_date TEXT NOT NULL,
   quantity INTEGER NOT NULL DEFAULT 1,
   print_status TEXT NOT NULL DEFAULT 'بانتظار الطباعة'
  );
  CREATE TABLE IF NOT EXISTS app_sessions(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   started_at TEXT NOT NULL,
   ended_at TEXT
  );
 ").map_err(|e|e.to_string())?;
 if !has_column(&conn,"tailored_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN tailored_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"delivery_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN delivery_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 Ok(conn)
}

fn begin_session(app:&AppHandle)->Result<i64,String>{
 let conn=db(app)?;
 conn.execute(
  "UPDATE app_sessions SET ended_at=started_at WHERE ended_at IS NULL",
  []
 ).map_err(|e|e.to_string())?;
 conn.execute(
  "INSERT INTO app_sessions(started_at) VALUES(datetime('now','localtime'))",
  []
 ).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

fn finish_session(app:&AppHandle,id:i64)->Result<(),String>{
 let conn=db(app)?;
 conn.execute(
  "UPDATE app_sessions SET ended_at=datetime('now','localtime') WHERE id=?1 AND ended_at IS NULL",
  [id]
 ).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn dashboard_summary(app:AppHandle)->Result<Dashboard,String>{
 let conn=db(&app)?;
 let received_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE received_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let tailored_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE tailored_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let due_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE delivery_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(Dashboard{received_today,tailored_today,due_today})
}

#[tauri::command]
fn current_session(app:AppHandle)->Result<CurrentSession,String>{
 let conn=db(&app)?;
 conn.query_row(
  "SELECT started_at FROM app_sessions WHERE ended_at IS NULL ORDER BY id DESC LIMIT 1",
  [],
  |row|Ok(CurrentSession{started_at:row.get(0)?})
 ).map_err(|e|e.to_string())
}

#[tauri::command]
fn session_history(app:AppHandle)->Result<Vec<SessionEntry>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT started_at,ended_at FROM app_sessions WHERE ended_at IS NOT NULL ORDER BY id DESC LIMIT 30"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(SessionEntry{
  started_at:row.get(0)?,
  ended_at:row.get(1)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

fn main(){
 tauri::Builder::default()
  .setup(|app|{
   let session_id=begin_session(&app.handle()) .map_err(std::io::Error::other)?;
   app.manage(SessionState(Mutex::new(Some(session_id))));
   Ok(())
  })
  .on_window_event(|window,event|{
   if let WindowEvent::CloseRequested{..}=event{
    let state=window.state::<SessionState>();
    if let Ok(mut session_id)=state.0.lock(){
     if let Some(id)=session_id.take(){
      let _=finish_session(&window.app_handle(),id);
     }
    }
   }
  })
  .invoke_handler(tauri::generate_handler![dashboard_summary,current_session,session_history])
  .run(tauri::generate_context!())
  .expect("تعذر تشغيل TAILOR");
}