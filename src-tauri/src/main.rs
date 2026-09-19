use rusqlite::{params,Connection};
use serde::{Deserialize,Serialize};
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

#[derive(Serialize)]
struct Customer{id:i64,code:String,name:String,phone:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DesignOption{id:i64,category:String,name:String,image_data:String}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct InvoicePayload{
 id:Option<i64>,customer_id:i64,weight:String,delivery_date:String,day_count:i64,total_thobes:i64,
 total_price:String,paid_amount:String,payment_method:String,discount:String,notes:String,
 measurements_json:String,fabric_json:String,designs_json:String,details_json:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct InvoiceRecord{id:i64,invoice_number:String,created_at:String}

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
  CREATE TABLE IF NOT EXISTS customers(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   customer_code TEXT UNIQUE,
   name TEXT NOT NULL,
   phone TEXT NOT NULL,
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS design_options(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   category TEXT NOT NULL,
   name TEXT NOT NULL,
   image_data TEXT NOT NULL,
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS invoices(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   invoice_number TEXT UNIQUE,
   order_id INTEGER NOT NULL,
   customer_id INTEGER NOT NULL,
   created_at TEXT NOT NULL,
   updated_at TEXT NOT NULL,
   weight TEXT NOT NULL DEFAULT '',
   delivery_date TEXT NOT NULL DEFAULT '',
   day_count INTEGER NOT NULL DEFAULT 0,
   total_thobes INTEGER NOT NULL DEFAULT 1,
   total_price TEXT NOT NULL DEFAULT '',
   paid_amount TEXT NOT NULL DEFAULT '',
   payment_method TEXT NOT NULL DEFAULT 'كاش',
   discount TEXT NOT NULL DEFAULT '',
   notes TEXT NOT NULL DEFAULT '',
   measurements_json TEXT NOT NULL DEFAULT '{}',
   fabric_json TEXT NOT NULL DEFAULT '{}',
   designs_json TEXT NOT NULL DEFAULT '{}',
   details_json TEXT NOT NULL DEFAULT '{}',
   FOREIGN KEY(order_id) REFERENCES orders(id),
   FOREIGN KEY(customer_id) REFERENCES customers(id)
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
  "SELECT started_at,ended_at FROM app_sessions WHERE ended_at IS NOT NULL ORDER BY id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(SessionEntry{
  started_at:row.get(0)?,
  ended_at:row.get(1)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn create_customer(app:AppHandle,name:String,phone:String)->Result<Customer,String>{
 let name=name.trim().to_string();
 let phone=phone.trim().to_string();
 if name.is_empty(){return Err("اسم العميل مطلوب".into())}
 if phone.chars().filter(|character|character.is_ascii_digit()).count()<7{
  return Err("رقم الجوال غير صحيح".into())
 }
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 transaction.execute(
  "INSERT INTO customers(customer_code,name,phone,created_at) VALUES(NULL,?1,?2,datetime('now','localtime'))",
  params![&name,&phone]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 let code=format!("A{id}");
 transaction.execute("UPDATE customers SET customer_code=?1 WHERE id=?2",params![&code,id]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(Customer{id,code,name,phone})
}

#[tauri::command]
fn list_design_options(app:AppHandle)->Result<Vec<DesignOption>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT id,category,name,image_data FROM design_options ORDER BY category,name,id").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(DesignOption{id:row.get(0)?,category:row.get(1)?,name:row.get(2)?,image_data:row.get(3)?})).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_design_option(app:AppHandle,category:String,name:String,image_data:String)->Result<i64,String>{
 let category=category.trim();
 let name=name.trim();
 if category.is_empty()||name.is_empty(){return Err("القسم واسم النوع مطلوبان".into())}
 if !image_data.starts_with("data:image/"){return Err("ملف الصورة غير صالح".into())}
 let conn=db(&app)?;
 conn.execute(
  "INSERT INTO design_options(category,name,image_data,created_at) VALUES(?1,?2,?3,datetime('now','localtime'))",
  params![category,name,image_data]
 ).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn delete_design_option(app:AppHandle,id:i64)->Result<(),String>{
 let conn=db(&app)?;
 conn.execute("DELETE FROM design_options WHERE id=?1",[id]).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn save_invoice(app:AppHandle,payload:InvoicePayload)->Result<InvoiceRecord,String>{
 let mut conn=db(&app)?;
 if let Some(id)=payload.id{
  conn.execute(
   "UPDATE invoices SET weight=?1,delivery_date=?2,day_count=?3,total_thobes=?4,total_price=?5,paid_amount=?6,payment_method=?7,discount=?8,notes=?9,measurements_json=?10,fabric_json=?11,designs_json=?12,details_json=?13,updated_at=datetime('now','localtime') WHERE id=?14 AND customer_id=?15",
   params![&payload.weight,&payload.delivery_date,payload.day_count,payload.total_thobes,&payload.total_price,&payload.paid_amount,&payload.payment_method,&payload.discount,&payload.notes,&payload.measurements_json,&payload.fabric_json,&payload.designs_json,&payload.details_json,id,payload.customer_id]
  ).map_err(|e|e.to_string())?;
  let (invoice_number,created_at,order_id):(String,String,i64)=conn.query_row(
   "SELECT invoice_number,created_at,order_id FROM invoices WHERE id=?1 AND customer_id=?2",
   params![id,payload.customer_id],
   |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
  ).map_err(|e|e.to_string())?;
  conn.execute("UPDATE orders SET quantity=?1,delivery_date=NULLIF(?2,'') WHERE id=?3",params![payload.total_thobes,&payload.delivery_date,order_id]).map_err(|e|e.to_string())?;
  return Ok(InvoiceRecord{id,invoice_number,created_at})
 }
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 transaction.execute(
  "INSERT INTO orders(received_date,quantity,print_status,delivery_date) VALUES(date('now','localtime'),?1,'بانتظار الطباعة',NULLIF(?2,''))",
  params![payload.total_thobes,&payload.delivery_date]
 ).map_err(|e|e.to_string())?;
 let order_id=transaction.last_insert_rowid();
 transaction.execute(
  "INSERT INTO invoices(invoice_number,order_id,customer_id,created_at,updated_at,weight,delivery_date,day_count,total_thobes,total_price,paid_amount,payment_method,discount,notes,measurements_json,fabric_json,designs_json,details_json) VALUES(NULL,?1,?2,datetime('now','localtime'),datetime('now','localtime'),?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
  params![order_id,payload.customer_id,&payload.weight,&payload.delivery_date,payload.day_count,payload.total_thobes,&payload.total_price,&payload.paid_amount,&payload.payment_method,&payload.discount,&payload.notes,&payload.measurements_json,&payload.fabric_json,&payload.designs_json,&payload.details_json]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 let invoice_number=format!("INV-{id:06}");
 transaction.execute("UPDATE invoices SET invoice_number=?1 WHERE id=?2",params![&invoice_number,id]).map_err(|e|e.to_string())?;
 let created_at:String=transaction.query_row("SELECT created_at FROM invoices WHERE id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(InvoiceRecord{id,invoice_number,created_at})
}

fn main(){
 tauri::Builder::default()
  .setup(|app|{
   let session_id=begin_session(&app.handle()).map_err(std::io::Error::other)?;
   app.manage(SessionState(Mutex::new(Some(session_id))));
   Ok(())
  })
  .on_window_event(|window,event|{
   if let WindowEvent::CloseRequested{..}=event{
    let state=window.state::<SessionState>();
    let session_id=match state.0.lock(){
     Ok(mut guard)=>guard.take(),
     Err(_)=>None,
    };
    if let Some(id)=session_id{
     let _=finish_session(&window.app_handle(),id);
    }
   }
  })
  .invoke_handler(tauri::generate_handler![dashboard_summary,current_session,session_history,create_customer,list_design_options,add_design_option,delete_design_option,save_invoice])
  .run(tauri::generate_context!())
  .expect("تعذر تشغيل TAILOR");
}