use rusqlite::{params,Connection};
use serde::{Deserialize,Serialize};
use std::{fs,path::{Path,PathBuf},sync::Mutex};
use tauri::{AppHandle,Manager,WindowEvent};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Dashboard{received_today:i64,tailored_today:i64,ready_to_deliver:i64}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkBoardItem{
 order_id:i64,invoice_number:String,customer_name:String,customer_code:String,quantity:i64,status:String
}

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
struct CustomerSearchItem{
 id:i64,code:String,name:String,phone:String,invoice_count:i64,last_invoice_at:String
}

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

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SavedInvoice{
 id:i64,invoice_number:String,customer_id:i64,created_at:String,weight:String,
 delivery_date:String,day_count:i64,total_thobes:i64,total_price:String,
 paid_amount:String,payment_method:String,discount:String,notes:String,
 measurements_json:String,fabric_json:String,designs_json:String,details_json:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct StorageInfo{folder:String,database_path:String,is_custom:bool}

struct SessionState(Mutex<Option<i64>>);

fn default_data_dir(app:&AppHandle)->Result<PathBuf,String>{
 app.path().app_data_dir().map_err(|e|e.to_string())
}

fn storage_config_path(app:&AppHandle)->Result<PathBuf,String>{
 Ok(default_data_dir(app)?.join("storage-location.txt"))
}

fn data_dir(app:&AppHandle)->Result<(PathBuf,bool),String>{
 let default=default_data_dir(app)?;
 if let Ok(raw)=fs::read_to_string(storage_config_path(app)?){
  let folder=raw.trim();
  if !folder.is_empty(){return Ok((PathBuf::from(folder),true))}
 }
 Ok((default,false))
}

fn database_path(app:&AppHandle)->Result<PathBuf,String>{
 Ok(data_dir(app)?.0.join("tailor.sqlite"))
}

fn current_storage_info(app:&AppHandle)->Result<StorageInfo,String>{
 let (folder,is_custom)=data_dir(app)?;
 Ok(StorageInfo{
  database_path:folder.join("tailor.sqlite").display().to_string(),
  folder:folder.display().to_string(),
  is_custom,
 })
}

fn has_column(conn:&Connection,column:&str)->Result<bool,String>{
 let mut statement=conn.prepare("PRAGMA table_info(orders)").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|row.get::<_,String>(1)).map_err(|e|e.to_string())?;
 for row in rows{
  if row.map_err(|e|e.to_string())?==column{return Ok(true)}
 }
 Ok(false)
}

fn db(app:&AppHandle)->Result<Connection,String>{
 let dir=data_dir(app)?.0;
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
 if !has_column(&conn,"work_status")?{
  conn.execute("ALTER TABLE orders ADD COLUMN work_status TEXT NOT NULL DEFAULT 'انتظار القص'",[]).map_err(|e|e.to_string())?;
 }
 conn.execute_batch("
  UPDATE customers SET customer_code='__customer_' || id;
  UPDATE customers SET customer_code=CAST(id AS TEXT);
 UPDATE invoices SET invoice_number='__invoice_' || id;
 UPDATE invoices SET invoice_number=CAST(id AS TEXT);
  UPDATE design_options SET category='الكبك' WHERE category='الكباك';
 ").map_err(|e|e.to_string())?;
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

fn finish_session_at(path:&Path,id:i64)->Result<(),String>{
 let conn=Connection::open(path).map_err(|e|e.to_string())?;
 conn.execute(
  "UPDATE app_sessions SET ended_at=datetime('now','localtime') WHERE id=?1 AND ended_at IS NULL",
  [id]
 ).map_err(|e|e.to_string())?;
 Ok(())
}

fn validate_tailor_database(path:&Path)->Result<(),String>{
 let conn=Connection::open(path).map_err(|_|"تعذر فتح ملف البيانات المحدد".to_string())?;
 let tables:i64=conn.query_row(
  "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('customers','invoices')",
  [],
  |row|row.get(0)
 ).map_err(|_|"ملف البيانات المحدد غير صالح".to_string())?;
 if tables<2{return Err("المجلد لا يحتوي على قاعدة بيانات TAILOR صالحة".into())}
 Ok(())
}

#[tauri::command]
fn storage_info(app:AppHandle)->Result<StorageInfo,String>{
 current_storage_info(&app)
}

#[tauri::command]
fn set_storage_location(app:AppHandle,state:tauri::State<SessionState>,folder:String,mode:String)->Result<StorageInfo,String>{
 let folder=folder.trim();
 if folder.is_empty(){return Err("اختر مجلدًا صالحًا".into())}
 let target_dir=PathBuf::from(folder);
 fs::create_dir_all(&target_dir).map_err(|_|"تعذر إنشاء المجلد أو الكتابة داخله".to_string())?;
 let source_database=database_path(&app)?;
 let target_database=target_dir.join("tailor.sqlite");
 if source_database!=target_database{
  match mode.as_str(){
   "copy"=>{
    drop(db(&app)?);
    if target_database.exists(){return Err("يوجد ملف بيانات في هذا المجلد. استخدم خيار ربط نسخة موجودة أو اختر مجلدًا فارغًا".into())}
    fs::copy(&source_database,&target_database).map_err(|_|"تعذر نسخ قاعدة البيانات إلى المجلد الجديد".to_string())?;
   },
   "use"=>{
    if !target_database.exists(){return Err("لم يتم العثور على ملف tailor.sqlite داخل المجلد المحدد".into())}
    validate_tailor_database(&target_database)?;
   },
   _=>return Err("طريقة تغيير مكان البيانات غير صحيحة".into()),
  }
 }
 let default=default_data_dir(&app)?;
 fs::create_dir_all(&default).map_err(|e|e.to_string())?;
 fs::write(storage_config_path(&app)?,target_dir.display().to_string()).map_err(|_|"تعذر حفظ مكان البيانات الجديد".to_string())?;
 let previous_session=state.0.lock().map_err(|_|"تعذر تحديث جلسة التطبيق".to_string())?.take();
 if let Some(id)=previous_session{let _=finish_session_at(&source_database,id);}
 drop(db(&app)?);
 let next_session=begin_session(&app)?;
 *state.0.lock().map_err(|_|"تعذر بدء جلسة البيانات الجديدة".to_string())?=Some(next_session);
 current_storage_info(&app)
}

#[tauri::command]
fn dashboard_summary(app:AppHandle)->Result<Dashboard,String>{
 let conn=db(&app)?;
 let received_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE received_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let tailored_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE tailored_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let ready_to_deliver=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE work_status='في المحل بانتظار التسليم'",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(Dashboard{received_today,tailored_today,ready_to_deliver})
}

#[tauri::command]
fn work_board(app:AppHandle)->Result<Vec<WorkBoardItem>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT o.id,i.invoice_number,c.name,c.customer_code,o.quantity,o.work_status
   FROM orders o
   JOIN invoices i ON i.order_id=o.id
   JOIN customers c ON c.id=i.customer_id
   ORDER BY o.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(WorkBoardItem{
  order_id:row.get(0)?,invoice_number:row.get(1)?,customer_name:row.get(2)?,
  customer_code:row.get(3)?,quantity:row.get(4)?,status:row.get(5)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn advance_order_status(app:AppHandle,order_id:i64)->Result<(),String>{
 let conn=db(&app)?;
 let current:String=conn.query_row("SELECT work_status FROM orders WHERE id=?1",[order_id],|row|row.get(0)).map_err(|_|"تعذر العثور على طلب الثوب".to_string())?;
 let next=match current.as_str(){
  "انتظار القص"=>"عند الخياط",
  "عند الخياط"=>"في المغسلة",
  "في المغسلة"=>"في المحل بانتظار التسليم",
  "في المحل بانتظار التسليم"=>"تم التسليم",
  "تم التسليم"=>return Ok(()),
  _=>return Err("حالة الثوب غير معروفة".into()),
 };
 conn.execute(
  "UPDATE orders SET work_status=?1,tailored_date=CASE WHEN ?1='في المغسلة' AND tailored_date IS NULL THEN date('now','localtime') ELSE tailored_date END WHERE id=?2",
  params![next,order_id]
 ).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn retreat_order_status(app:AppHandle,order_id:i64)->Result<(),String>{
 let conn=db(&app)?;
 let current:String=conn.query_row("SELECT work_status FROM orders WHERE id=?1",[order_id],|row|row.get(0)).map_err(|_|"تعذر العثور على طلب الثوب".to_string())?;
 let previous=match current.as_str(){
  "انتظار القص"=>return Ok(()),
  "عند الخياط"=>"انتظار القص",
  "في المغسلة"=>"عند الخياط",
  "في المحل بانتظار التسليم"=>"في المغسلة",
  "تم التسليم"=>"في المحل بانتظار التسليم",
  _=>return Err("حالة الثوب غير معروفة".into()),
 };
 conn.execute(
  "UPDATE orders SET work_status=?1,tailored_date=CASE WHEN work_status='في المغسلة' AND ?1='عند الخياط' THEN NULL ELSE tailored_date END WHERE id=?2",
  params![previous,order_id]
 ).map_err(|e|e.to_string())?;
 Ok(())
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
 let code=id.to_string();
 transaction.execute("UPDATE customers SET customer_code=?1 WHERE id=?2",params![&code,id]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(Customer{id,code,name,phone})
}

#[tauri::command]
fn search_customers(app:AppHandle,query:String)->Result<Vec<CustomerSearchItem>,String>{
 let conn=db(&app)?;
 let query=query.trim().to_string();
 let escaped=query.replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_");
 let contains_pattern=format!("%{}%",escaped);
 let prefix_pattern=format!("{}%",escaped);
 let word_prefix_pattern=format!("% {}%",escaped);
 let mut statement=conn.prepare(
  "SELECT c.id,c.customer_code,c.name,c.phone,COUNT(i.id),COALESCE(MAX(i.created_at),'')
   FROM customers c
   LEFT JOIN invoices i ON i.customer_id=c.id
   WHERE ?1=''
      OR c.name LIKE ?2 ESCAPE '\\' COLLATE NOCASE
      OR c.customer_code LIKE ?2 ESCAPE '\\'
      OR c.phone LIKE ?2 ESCAPE '\\'
   GROUP BY c.id,c.customer_code,c.name,c.phone
   ORDER BY CASE
     WHEN ?1='' THEN 4
     WHEN c.name=?1 COLLATE NOCASE THEN 0
     WHEN c.name LIKE ?3 ESCAPE '\\' COLLATE NOCASE THEN 1
     WHEN c.name LIKE ?4 ESCAPE '\\' COLLATE NOCASE THEN 2
     ELSE 3
   END,
   length(c.name) ASC,CAST(c.customer_code AS INTEGER) DESC,c.id DESC
   LIMIT 200"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map(params![&query,&contains_pattern,&prefix_pattern,&word_prefix_pattern],|row|Ok(CustomerSearchItem{
  id:row.get(0)?,code:row.get(1)?,name:row.get(2)?,phone:row.get(3)?,
  invoice_count:row.get(4)?,last_invoice_at:row.get(5)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn customer_invoices(app:AppHandle,customer_id:i64)->Result<Vec<SavedInvoice>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT id,invoice_number,customer_id,created_at,weight,delivery_date,day_count,total_thobes,
          total_price,paid_amount,payment_method,discount,notes,measurements_json,fabric_json,
          designs_json,details_json
   FROM invoices WHERE customer_id=?1 ORDER BY datetime(created_at) DESC,id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([customer_id],|row|Ok(SavedInvoice{
  id:row.get(0)?,invoice_number:row.get(1)?,customer_id:row.get(2)?,created_at:row.get(3)?,
  weight:row.get(4)?,delivery_date:row.get(5)?,day_count:row.get(6)?,total_thobes:row.get(7)?,
  total_price:row.get(8)?,paid_amount:row.get(9)?,payment_method:row.get(10)?,discount:row.get(11)?,
  notes:row.get(12)?,measurements_json:row.get(13)?,fabric_json:row.get(14)?,
  designs_json:row.get(15)?,details_json:row.get(16)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
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
 let invoice_number=id.to_string();
 transaction.execute("UPDATE invoices SET invoice_number=?1 WHERE id=?2",params![&invoice_number,id]).map_err(|e|e.to_string())?;
 let created_at:String=transaction.query_row("SELECT created_at FROM invoices WHERE id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(InvoiceRecord{id,invoice_number,created_at})
}

fn main(){
 tauri::Builder::default()
  .plugin(tauri_plugin_dialog::init())
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
  .invoke_handler(tauri::generate_handler![dashboard_summary,work_board,advance_order_status,retreat_order_status,current_session,session_history,storage_info,set_storage_location,create_customer,search_customers,customer_invoices,list_design_options,add_design_option,delete_design_option,save_invoice])
  .run(tauri::generate_context!())
  .expect("تعذر تشغيل TAILOR");
}
