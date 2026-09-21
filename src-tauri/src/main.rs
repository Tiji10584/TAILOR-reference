use rusqlite::{params,Connection,OptionalExtension};
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
 id:i64,code:String,name:String,phone:String,invoice_count:i64,last_invoice_at:String,outstanding:f64
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DesignOption{id:i64,category:String,name:String,image_data:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Supplier{id:i64,name:String,phone:String,notes:String,fabric_count:i64,total_paid:f64,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SupplierPayment{id:i64,supplier_id:i64,supplier_name:String,amount:f64,payment_method:String,notes:String,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct FabricItem{
 id:i64,supplier_id:Option<i64>,supplier_name:String,name:String,color:String,
 stock_meters:f64,purchase_price:f64,sale_price:f64,created_at:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct FabricMovement{
 id:i64,movement_type:String,meters:f64,balance_after:f64,entry_unit:String,carton_count:f64,
 meters_per_carton:f64,total_cost:f64,unit_cost:f64,notes:String,created_at:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ShopNote{id:i64,title:String,details:String,due_date:String,is_done:bool,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WhatsappCampaign{id:i64,title:String,message:String,recipient_count:i64,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct FinancialEntry{id:i64,entry_type:String,description:String,amount:f64,payment_method:String,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DebtInvoice{invoice_id:i64,invoice_number:String,customer_name:String,phone:String,total:f64,paid:f64,discount:f64,remaining:f64}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct FinancialOverview{
 today_sales:f64,today_received:f64,today_extra_income:f64,today_expenses:f64,total_outstanding:f64,
 month_sales:f64,month_received:f64,month_extra_income:f64,month_expenses:f64,
 entries:Vec<FinancialEntry>,debts:Vec<DebtInvoice>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DailyReport{
 report_date:String,period:String,period_label:String,new_customers:i64,invoices:i64,thobes:i64,invoice_sales:f64,invoice_received:f64,
 extra_income:f64,expenses:f64,delivered:i64,fabric_used:f64,fabric_sold:f64
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct AppSettings{shop_name:String,owner_name:String,finance_pin_set:bool,app_pin_set:bool,theme:String}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct FabricUsagePayload{fabric_id:i64,thobe_index:i64,meters:f64}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct InvoicePayload{
 id:Option<i64>,customer_id:i64,weight:String,delivery_date:String,day_count:i64,total_thobes:i64,
 total_price:String,paid_amount:String,payment_method:String,discount:String,notes:String,
 measurements_json:String,fabric_json:String,designs_json:String,details_json:String,
 fabric_usages:Vec<FabricUsagePayload>,confirm_low_stock:bool
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

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct ExtraTransactionPayload{
 transaction_type:String,customer_name:String,customer_phone:String,fabric_id:Option<i64>,
 quantity:i64,meters:f64,description:String,total_price:f64,payment_method:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ExtraTransaction{
 id:i64,transaction_type:String,customer_name:String,customer_phone:String,fabric_id:Option<i64>,
 fabric_name:String,fabric_color:String,quantity:i64,meters:f64,description:String,total_price:f64,
 payment_method:String,created_at:String
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

fn has_column(conn:&Connection,table:&str,column:&str)->Result<bool,String>{
 let mut statement=conn.prepare(&format!("PRAGMA table_info({})",table)).map_err(|e|e.to_string())?;
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
  CREATE TABLE IF NOT EXISTS suppliers(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   name TEXT NOT NULL,
   phone TEXT NOT NULL DEFAULT '',
   notes TEXT NOT NULL DEFAULT '',
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS fabrics(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   supplier_id INTEGER,
   name TEXT NOT NULL,
   color TEXT NOT NULL,
   stock_meters REAL NOT NULL DEFAULT 0,
   purchase_price REAL NOT NULL DEFAULT 0,
   sale_price REAL NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL,
   FOREIGN KEY(supplier_id) REFERENCES suppliers(id)
  );
  CREATE TABLE IF NOT EXISTS fabric_movements(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   fabric_id INTEGER NOT NULL,
   movement_type TEXT NOT NULL,
   meters REAL NOT NULL,
   balance_after REAL NOT NULL,
   reference_type TEXT NOT NULL DEFAULT '',
   reference_id INTEGER,
   notes TEXT NOT NULL DEFAULT '',
   created_at TEXT NOT NULL,
   FOREIGN KEY(fabric_id) REFERENCES fabrics(id)
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
  CREATE TABLE IF NOT EXISTS invoice_fabric_usage(
   invoice_id INTEGER NOT NULL,
   fabric_id INTEGER NOT NULL,
   thobe_index INTEGER NOT NULL,
   meters REAL NOT NULL,
   PRIMARY KEY(invoice_id,thobe_index),
   FOREIGN KEY(invoice_id) REFERENCES invoices(id),
   FOREIGN KEY(fabric_id) REFERENCES fabrics(id)
  );
  CREATE TABLE IF NOT EXISTS extra_transactions(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   transaction_type TEXT NOT NULL,
   customer_name TEXT NOT NULL,
   customer_phone TEXT NOT NULL DEFAULT '',
   fabric_id INTEGER,
   quantity INTEGER NOT NULL DEFAULT 1,
   meters REAL NOT NULL DEFAULT 0,
   description TEXT NOT NULL DEFAULT '',
   total_price REAL NOT NULL DEFAULT 0,
   payment_method TEXT NOT NULL DEFAULT 'كاش',
   created_at TEXT NOT NULL,
   FOREIGN KEY(fabric_id) REFERENCES fabrics(id)
  );
  CREATE TABLE IF NOT EXISTS shop_notes(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   title TEXT NOT NULL,
   details TEXT NOT NULL DEFAULT '',
   due_date TEXT NOT NULL DEFAULT '',
   is_done INTEGER NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS whatsapp_campaigns(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   title TEXT NOT NULL,
   message TEXT NOT NULL,
   recipient_count INTEGER NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS financial_entries(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   entry_type TEXT NOT NULL,
   invoice_id INTEGER,
   description TEXT NOT NULL DEFAULT '',
   amount REAL NOT NULL,
   payment_method TEXT NOT NULL DEFAULT 'كاش',
   created_at TEXT NOT NULL,
   FOREIGN KEY(invoice_id) REFERENCES invoices(id)
  );
  CREATE TABLE IF NOT EXISTS supplier_payments(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   supplier_id INTEGER NOT NULL,
   amount REAL NOT NULL,
   payment_method TEXT NOT NULL DEFAULT 'كاش',
   notes TEXT NOT NULL DEFAULT '',
   created_at TEXT NOT NULL,
   FOREIGN KEY(supplier_id) REFERENCES suppliers(id)
  );
  CREATE TABLE IF NOT EXISTS app_settings(
   setting_key TEXT PRIMARY KEY,
   setting_value TEXT NOT NULL DEFAULT ''
  );
 ").map_err(|e|e.to_string())?;
 if !has_column(&conn,"orders","tailored_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN tailored_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"orders","delivery_date")?{
  conn.execute("ALTER TABLE orders ADD COLUMN delivery_date TEXT",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"orders","work_status")?{
  conn.execute("ALTER TABLE orders ADD COLUMN work_status TEXT NOT NULL DEFAULT 'انتظار القص'",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"orders","delivered_at")?{
  conn.execute("ALTER TABLE orders ADD COLUMN delivered_at TEXT",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"fabric_movements","entry_unit")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN entry_unit TEXT NOT NULL DEFAULT 'متر'",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","carton_count")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN carton_count REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","meters_per_carton")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN meters_per_carton REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","total_cost")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN total_cost REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","unit_cost")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN unit_cost REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 conn.execute_batch("
  UPDATE customers SET customer_code='__customer_' || id;
  UPDATE customers SET customer_code=CAST(id AS TEXT);
 UPDATE invoices SET invoice_number='__invoice_' || id;
 UPDATE invoices SET invoice_number=CAST(id AS TEXT);
  UPDATE design_options SET category='الكبك' WHERE category='الكباك';
  INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at)
  SELECT 'دفعة فاتورة',i.id,'دفعة أولية للفاتورة ' || i.invoice_number,CAST(NULLIF(i.paid_amount,'') AS REAL),i.payment_method,i.created_at
  FROM invoices i
  WHERE CAST(NULLIF(i.paid_amount,'') AS REAL)>0
    AND NOT EXISTS(SELECT 1 FROM financial_entries f WHERE f.invoice_id=i.id AND f.entry_type='دفعة فاتورة');
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
  "UPDATE orders SET work_status=?1,tailored_date=CASE WHEN ?1='في المغسلة' AND tailored_date IS NULL THEN date('now','localtime') ELSE tailored_date END,delivered_at=CASE WHEN ?1='تم التسليم' THEN datetime('now','localtime') ELSE delivered_at END WHERE id=?2",
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
  "UPDATE orders SET work_status=?1,tailored_date=CASE WHEN work_status='في المغسلة' AND ?1='عند الخياط' THEN NULL ELSE tailored_date END,delivered_at=CASE WHEN work_status='تم التسليم' THEN NULL ELSE delivered_at END WHERE id=?2",
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
  "SELECT c.id,c.customer_code,c.name,c.phone,COUNT(i.id),COALESCE(MAX(i.created_at),''),
          COALESCE(SUM(MAX(0,CAST(NULLIF(i.total_price,'') AS REAL)-CAST(NULLIF(i.paid_amount,'') AS REAL)-CAST(NULLIF(i.discount,'') AS REAL))),0)
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
  invoice_count:row.get(4)?,last_invoice_at:row.get(5)?,outstanding:row.get(6)?,
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
fn list_suppliers(app:AppHandle)->Result<Vec<Supplier>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT s.id,s.name,s.phone,s.notes,COUNT(f.id),
          COALESCE((SELECT SUM(p.amount) FROM supplier_payments p WHERE p.supplier_id=s.id),0),s.created_at
   FROM suppliers s LEFT JOIN fabrics f ON f.supplier_id=s.id
   GROUP BY s.id,s.name,s.phone,s.notes,s.created_at ORDER BY s.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(Supplier{
  id:row.get(0)?,name:row.get(1)?,phone:row.get(2)?,notes:row.get(3)?,
  fabric_count:row.get(4)?,total_paid:row.get(5)?,created_at:row.get(6)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_supplier(app:AppHandle,name:String,phone:String,notes:String)->Result<i64,String>{
 let name=name.trim();
 if name.is_empty(){return Err("اسم المورد مطلوب".into())}
 let conn=db(&app)?;
 conn.execute(
  "INSERT INTO suppliers(name,phone,notes,created_at) VALUES(?1,?2,?3,datetime('now','localtime'))",
  params![name,phone.trim(),notes.trim()]
 ).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn list_supplier_payments(app:AppHandle,supplier_id:Option<i64>)->Result<Vec<SupplierPayment>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT p.id,p.supplier_id,s.name,p.amount,p.payment_method,p.notes,p.created_at
   FROM supplier_payments p JOIN suppliers s ON s.id=p.supplier_id
   WHERE ?1 IS NULL OR p.supplier_id=?1 ORDER BY p.id DESC LIMIT 200"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([supplier_id],|row|Ok(SupplierPayment{
  id:row.get(0)?,supplier_id:row.get(1)?,supplier_name:row.get(2)?,amount:row.get(3)?,
  payment_method:row.get(4)?,notes:row.get(5)?,created_at:row.get(6)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_supplier_payment(app:AppHandle,supplier_id:i64,amount:f64,payment_method:String,notes:String)->Result<SupplierPayment,String>{
 if !amount.is_finite()||amount<=0.0{return Err("أدخل مبلغ الدفعة".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let supplier_name:String=transaction.query_row("SELECT name FROM suppliers WHERE id=?1",[supplier_id],|row|row.get(0)).map_err(|_|"المورد غير موجود".to_string())?;
 transaction.execute(
  "INSERT INTO supplier_payments(supplier_id,amount,payment_method,notes,created_at) VALUES(?1,?2,?3,?4,datetime('now','localtime'))",
  params![supplier_id,amount,payment_method.trim(),notes.trim()]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 transaction.execute(
  "INSERT INTO financial_entries(entry_type,description,amount,payment_method,created_at) VALUES('دفعة مورد',?1,?2,?3,datetime('now','localtime'))",
  params![format!("دفعة للمورد {}",supplier_name),amount,payment_method.trim()]
 ).map_err(|e|e.to_string())?;
 let created_at:String=transaction.query_row("SELECT created_at FROM supplier_payments WHERE id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(SupplierPayment{id,supplier_id,supplier_name,amount,payment_method,notes,created_at})
}

#[tauri::command]
fn list_fabrics(app:AppHandle)->Result<Vec<FabricItem>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT f.id,f.supplier_id,COALESCE(s.name,''),f.name,f.color,f.stock_meters,
          f.purchase_price,f.sale_price,f.created_at
   FROM fabrics f LEFT JOIN suppliers s ON s.id=f.supplier_id
   ORDER BY f.name,f.color,f.id"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(FabricItem{
  id:row.get(0)?,supplier_id:row.get(1)?,supplier_name:row.get(2)?,name:row.get(3)?,color:row.get(4)?,
  stock_meters:row.get(5)?,purchase_price:row.get(6)?,sale_price:row.get(7)?,created_at:row.get(8)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_fabric(app:AppHandle,supplier_id:Option<i64>,name:String,color:String,entry_unit:String,meter_quantity:f64,carton_count:f64,meters_per_carton:f64,purchase_amount:f64,sale_price:f64)->Result<i64,String>{
 let name=name.trim();let color=color.trim();
 if name.is_empty()||color.is_empty(){return Err("اسم القماش واللون مطلوبان".into())}
 let (stock_meters,purchase_price,total_cost,normalized_unit)=supply_calculation(&entry_unit,meter_quantity,carton_count,meters_per_carton,purchase_amount)?;
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let existing=transaction.query_row(
  "SELECT id,stock_meters,purchase_price FROM fabrics
   WHERE lower(trim(name))=lower(?1) AND lower(trim(color))=lower(?2)
     AND ((supplier_id IS NULL AND ?3 IS NULL) OR supplier_id=?3)
   ORDER BY id LIMIT 1",
  params![name,color,supplier_id],
  |row|Ok((row.get::<_,i64>(0)?,row.get::<_,f64>(1)?,row.get::<_,f64>(2)?))
 ).optional().map_err(|e|e.to_string())?;
 if let Some((id,current_stock,current_cost))=existing{
  let balance=current_stock+stock_meters;
  let average_cost=if total_cost>0.0&&balance>0.0{((current_stock*current_cost)+total_cost)/balance}else{current_cost};
  transaction.execute(
   "UPDATE fabrics SET stock_meters=?1,purchase_price=?2,sale_price=CASE WHEN ?3>0 THEN ?3 ELSE sale_price END WHERE id=?4",
   params![balance,average_cost,sale_price.max(0.0),id]
  ).map_err(|e|e.to_string())?;
  transaction.execute(
   "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,entry_unit,carton_count,meters_per_carton,total_cost,unit_cost,notes,created_at)
    VALUES(?1,'توريد ودمج',?2,?3,?4,?5,?6,?7,?8,'دُمج مع القماش الموجود',datetime('now','localtime'))",
   params![id,stock_meters,balance,normalized_unit,carton_count.max(0.0),meters_per_carton.max(0.0),total_cost,purchase_price]
  ).map_err(|e|e.to_string())?;
  transaction.commit().map_err(|e|e.to_string())?;
  return Ok(id)
 }
 transaction.execute(
  "INSERT INTO fabrics(supplier_id,name,color,stock_meters,purchase_price,sale_price,created_at)
   VALUES(?1,?2,?3,?4,?5,?6,datetime('now','localtime'))",
  params![supplier_id,name,color,stock_meters,purchase_price,sale_price.max(0.0)]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 if stock_meters>0.0{
  transaction.execute(
   "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,entry_unit,carton_count,meters_per_carton,total_cost,unit_cost,notes,created_at)
    VALUES(?1,'إضافة أولية',?2,?2,?3,?4,?5,?6,?7,'تسجيل القماش',datetime('now','localtime'))",
   params![id,stock_meters,normalized_unit,carton_count.max(0.0),meters_per_carton.max(0.0),total_cost,purchase_price]
  ).map_err(|e|e.to_string())?;
 }
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(id)
}

fn supply_calculation(entry_unit:&str,meter_quantity:f64,carton_count:f64,meters_per_carton:f64,purchase_amount:f64)->Result<(f64,f64,f64,String),String>{
 if !purchase_amount.is_finite()||purchase_amount<0.0{return Err("سعر الشراء غير صحيح".into())}
 if entry_unit.trim()=="كرتون"{
  if !carton_count.is_finite()||carton_count<=0.0{return Err("أدخل عدد الكراتين".into())}
  if !meters_per_carton.is_finite()||meters_per_carton<=0.0{return Err("أدخل عدد الأمتار داخل الكرتون".into())}
  let meters=carton_count*meters_per_carton;
  let total_cost=carton_count*purchase_amount;
  let unit_cost=if meters>0.0{total_cost/meters}else{0.0};
  return Ok((meters,unit_cost,total_cost,"كرتون".into()))
 }
 if !meter_quantity.is_finite()||meter_quantity<=0.0{return Err("أدخل كمية القماش بالمتر".into())}
 Ok((meter_quantity,purchase_amount,meter_quantity*purchase_amount,"متر".into()))
}

#[tauri::command]
fn restock_fabric(app:AppHandle,fabric_id:i64,entry_unit:String,meter_quantity:f64,carton_count:f64,meters_per_carton:f64,purchase_amount:f64,notes:String)->Result<(),String>{
 let (meters,purchase_price,total_cost,normalized_unit)=supply_calculation(&entry_unit,meter_quantity,carton_count,meters_per_carton,purchase_amount)?;
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let current:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|_|"القماش غير موجود".to_string())?;
 let balance=current+meters;
 transaction.execute("UPDATE fabrics SET stock_meters=?1,purchase_price=CASE WHEN ?2>0 THEN ?2 ELSE purchase_price END WHERE id=?3",params![balance,purchase_price,fabric_id]).map_err(|e|e.to_string())?;
 transaction.execute(
  "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,entry_unit,carton_count,meters_per_carton,total_cost,unit_cost,notes,created_at)
   VALUES(?1,'توريد',?2,?3,?4,?5,?6,?7,?8,?9,datetime('now','localtime'))",
  params![fabric_id,meters,balance,normalized_unit,carton_count.max(0.0),meters_per_carton.max(0.0),total_cost,purchase_price,notes.trim()]
 ).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())
}

#[tauri::command]
fn fabric_movements(app:AppHandle,fabric_id:i64)->Result<Vec<FabricMovement>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT id,movement_type,meters,balance_after,entry_unit,carton_count,meters_per_carton,total_cost,unit_cost,notes,created_at
   FROM fabric_movements WHERE fabric_id=?1 ORDER BY id DESC LIMIT 100"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([fabric_id],|row|Ok(FabricMovement{
  id:row.get(0)?,movement_type:row.get(1)?,meters:row.get(2)?,balance_after:row.get(3)?,entry_unit:row.get(4)?,
  carton_count:row.get(5)?,meters_per_carton:row.get(6)?,total_cost:row.get(7)?,unit_cost:row.get(8)?,notes:row.get(9)?,created_at:row.get(10)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn list_notes(app:AppHandle)->Result<Vec<ShopNote>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT id,title,details,due_date,is_done,created_at FROM shop_notes ORDER BY is_done ASC,CASE WHEN due_date='' THEN 1 ELSE 0 END,due_date ASC,id DESC").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ShopNote{id:row.get(0)?,title:row.get(1)?,details:row.get(2)?,due_date:row.get(3)?,is_done:row.get::<_,i64>(4)?!=0,created_at:row.get(5)?})).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_note(app:AppHandle,title:String,details:String,due_date:String)->Result<i64,String>{
 if title.trim().is_empty(){return Err("عنوان الملاحظة مطلوب".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO shop_notes(title,details,due_date,created_at) VALUES(?1,?2,?3,datetime('now','localtime'))",params![title.trim(),details.trim(),due_date.trim()]).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn toggle_note(app:AppHandle,id:i64,is_done:bool)->Result<(),String>{
 let conn=db(&app)?;
 conn.execute("UPDATE shop_notes SET is_done=?1 WHERE id=?2",params![if is_done{1}else{0},id]).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn delete_note(app:AppHandle,id:i64)->Result<(),String>{
 let conn=db(&app)?;
 conn.execute("DELETE FROM shop_notes WHERE id=?1",[id]).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn list_whatsapp_campaigns(app:AppHandle)->Result<Vec<WhatsappCampaign>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT id,title,message,recipient_count,created_at FROM whatsapp_campaigns ORDER BY id DESC LIMIT 50").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(WhatsappCampaign{id:row.get(0)?,title:row.get(1)?,message:row.get(2)?,recipient_count:row.get(3)?,created_at:row.get(4)?})).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn save_whatsapp_campaign(app:AppHandle,title:String,message:String,recipient_count:i64)->Result<i64,String>{
 if title.trim().is_empty()||message.trim().is_empty(){return Err("عنوان الإعلان ونص الرسالة مطلوبان".into())}
 if recipient_count<1{return Err("اختر عميلًا واحدًا على الأقل".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO whatsapp_campaigns(title,message,recipient_count,created_at) VALUES(?1,?2,?3,datetime('now','localtime'))",params![title.trim(),message.trim(),recipient_count]).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

fn parse_money(value:&str)->f64{value.trim().parse::<f64>().unwrap_or(0.0)}

#[tauri::command]
fn financial_overview(app:AppHandle)->Result<FinancialOverview,String>{
 let conn=db(&app)?;
 let today_sales=conn.query_row("SELECT COALESCE(SUM(CAST(NULLIF(total_price,'') AS REAL)),0) FROM invoices WHERE date(created_at)=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let today_received:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دفعة فاتورة' AND date(created_at)=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let registered_extra:f64=conn.query_row("SELECT COALESCE(SUM(total_price),0) FROM extra_transactions WHERE date(created_at)=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let manual_income:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دخل يدوي' AND date(created_at)=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let today_extra_income=registered_extra+manual_income;
 let today_expenses=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type IN ('مصروف','دفعة مورد') AND date(created_at)=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let month_sales=conn.query_row("SELECT COALESCE(SUM(CAST(NULLIF(total_price,'') AS REAL)),0) FROM invoices WHERE date(created_at)>=date('now','localtime','start of month') AND date(created_at)<date('now','localtime','start of month','+1 month')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let month_received:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دفعة فاتورة' AND date(created_at)>=date('now','localtime','start of month') AND date(created_at)<date('now','localtime','start of month','+1 month')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let month_registered_extra:f64=conn.query_row("SELECT COALESCE(SUM(total_price),0) FROM extra_transactions WHERE date(created_at)>=date('now','localtime','start of month') AND date(created_at)<date('now','localtime','start of month','+1 month')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let month_manual_income:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دخل يدوي' AND date(created_at)>=date('now','localtime','start of month') AND date(created_at)<date('now','localtime','start of month','+1 month')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let month_extra_income=month_registered_extra+month_manual_income;
 let month_expenses=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type IN ('مصروف','دفعة مورد') AND date(created_at)>=date('now','localtime','start of month') AND date(created_at)<date('now','localtime','start of month','+1 month')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let total_outstanding=conn.query_row("SELECT COALESCE(SUM(MAX(0,CAST(NULLIF(total_price,'') AS REAL)-CAST(NULLIF(paid_amount,'') AS REAL)-CAST(NULLIF(discount,'') AS REAL))),0) FROM invoices",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let entries={
  let mut statement=conn.prepare("SELECT id,entry_type,description,amount,payment_method,created_at FROM financial_entries ORDER BY id DESC LIMIT 100").map_err(|e|e.to_string())?;
  let rows=statement.query_map([],|row|Ok(FinancialEntry{id:row.get(0)?,entry_type:row.get(1)?,description:row.get(2)?,amount:row.get(3)?,payment_method:row.get(4)?,created_at:row.get(5)?})).map_err(|e|e.to_string())?;
  rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?
 };
 let debts={
  let mut statement=conn.prepare("SELECT i.id,i.invoice_number,c.name,c.phone,i.total_price,i.paid_amount,i.discount FROM invoices i JOIN customers c ON c.id=i.customer_id ORDER BY i.id DESC").map_err(|e|e.to_string())?;
  let rows=statement.query_map([],|row|{
   let total_text:String=row.get(4)?;let paid_text:String=row.get(5)?;let discount_text:String=row.get(6)?;
   let total=parse_money(&total_text);let paid=parse_money(&paid_text);let discount=parse_money(&discount_text);
   Ok(DebtInvoice{invoice_id:row.get(0)?,invoice_number:row.get(1)?,customer_name:row.get(2)?,phone:row.get(3)?,total,paid,discount,remaining:(total-paid-discount).max(0.0)})
  }).map_err(|e|e.to_string())?;
  rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.into_iter().filter(|item|item.remaining>0.0001).collect()
 };
 Ok(FinancialOverview{today_sales,today_received,today_extra_income,today_expenses,total_outstanding,month_sales,month_received,month_extra_income,month_expenses,entries,debts})
}

#[tauri::command]
fn add_financial_entry(app:AppHandle,entry_type:String,description:String,amount:f64,payment_method:String)->Result<i64,String>{
 if entry_type!="مصروف"&&entry_type!="دخل يدوي"{return Err("نوع الحركة المالية غير صحيح".into())}
 if !amount.is_finite()||amount<=0.0{return Err("أدخل مبلغًا أكبر من صفر".into())}
 if description.trim().is_empty(){return Err("اكتب وصف الحركة المالية".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO financial_entries(entry_type,description,amount,payment_method,created_at) VALUES(?1,?2,?3,?4,datetime('now','localtime'))",params![entry_type,description.trim(),amount,payment_method]).map_err(|e|e.to_string())?;
 Ok(conn.last_insert_rowid())
}

#[tauri::command]
fn record_invoice_payment(app:AppHandle,invoice_id:i64,amount:f64,payment_method:String)->Result<(),String>{
 if !amount.is_finite()||amount<=0.0{return Err("أدخل مبلغ الدفعة".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let (invoice_number,customer_name,total_text,paid_text,discount_text):(String,String,String,String,String)=transaction.query_row(
  "SELECT i.invoice_number,c.name,i.total_price,i.paid_amount,i.discount FROM invoices i JOIN customers c ON c.id=i.customer_id WHERE i.id=?1",[invoice_id],
  |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))
 ).map_err(|_|"الفاتورة غير موجودة".to_string())?;
 let total=parse_money(&total_text);let paid=parse_money(&paid_text);let discount=parse_money(&discount_text);let remaining=(total-paid-discount).max(0.0);
 if amount>remaining+0.0001{return Err(format!("المتبقي على الفاتورة {:.2} ريال فقط",remaining))}
 let new_paid=paid+amount;
 transaction.execute("UPDATE invoices SET paid_amount=?1,updated_at=datetime('now','localtime') WHERE id=?2",params![format!("{:.2}",new_paid),invoice_id]).map_err(|e|e.to_string())?;
 transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![invoice_id,format!("فاتورة {} — {}",invoice_number,customer_name),amount,payment_method]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())
}

#[tauri::command]
fn daily_report(app:AppHandle,report_date:String,period:String)->Result<DailyReport,String>{
 let conn=db(&app)?;let day=report_date.trim();
 if day.is_empty(){return Err("اختر تاريخ التقرير".into())}
 if day.len()<10{return Err("تاريخ التقرير غير صحيح".into())}
 let (start,end,period_label)=match period.as_str(){
  "شهري"=>(format!("{}-01",&day[..7]),"month".to_string(),"تقرير مالي شهري".to_string()),
  "سنوي"=>(format!("{}-01-01",&day[..4]),"year".to_string(),"تقرير مالي سنوي".to_string()),
  _=>(day.to_string(),"day".to_string(),"تقرير مالي يومي".to_string()),
 };
 let end_date=match end.as_str(){"month"=>conn.query_row("SELECT date(?1,'+1 month')",[&start],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?,"year"=>conn.query_row("SELECT date(?1,'+1 year')",[&start],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?,_=>conn.query_row("SELECT date(?1,'+1 day')",[&start],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?};
 let new_customers=conn.query_row("SELECT COUNT(*) FROM customers WHERE date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let (invoices,thobes,invoice_sales):(i64,i64,f64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(total_thobes),0),COALESCE(SUM(CAST(NULLIF(total_price,'') AS REAL)),0) FROM invoices WHERE date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(|e|e.to_string())?;
 let invoice_received:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دفعة فاتورة' AND date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let registered_extra:f64=conn.query_row("SELECT COALESCE(SUM(total_price),0) FROM extra_transactions WHERE date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let manual_income:f64=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type='دخل يدوي' AND date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let extra_income=registered_extra+manual_income;
 let expenses=conn.query_row("SELECT COALESCE(SUM(amount),0) FROM financial_entries WHERE entry_type IN ('مصروف','دفعة مورد') AND date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let delivered=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE work_status='تم التسليم' AND date(delivered_at)>=date(?1) AND date(delivered_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let fabric_used=conn.query_row("SELECT COALESCE(ABS(SUM(meters)),0) FROM fabric_movements WHERE movement_type='تفصيل ثوب' AND date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 let fabric_sold=conn.query_row("SELECT COALESCE(ABS(SUM(meters)),0) FROM fabric_movements WHERE movement_type='بيع قماش' AND date(created_at)>=date(?1) AND date(created_at)<date(?2)",params![&start,&end_date],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(DailyReport{report_date:day.into(),period,period_label,new_customers,invoices,thobes,invoice_sales,invoice_received,extra_income,expenses,delivered,fabric_used,fabric_sold})
}

#[tauri::command]
fn get_app_settings(app:AppHandle)->Result<AppSettings,String>{
 let conn=db(&app)?;
 let value=|key:&str|->String{conn.query_row("SELECT setting_value FROM app_settings WHERE setting_key=?1",[key],|row|row.get(0)).unwrap_or_default()};
 let finance_pin=value("finance_pin");
 let app_pin=value("app_pin");
 let saved_theme=value("theme");
 Ok(AppSettings{shop_name:value("shop_name"),owner_name:value("owner_name"),finance_pin_set:!finance_pin.is_empty(),app_pin_set:!app_pin.is_empty(),theme:if saved_theme=="light"{"light".into()}else{"dark".into()}})
}

#[tauri::command]
fn save_app_settings(app:AppHandle,shop_name:String,owner_name:String,finance_pin:String,app_pin:String)->Result<AppSettings,String>{
 let pin=finance_pin.trim();let entry_pin=app_pin.trim();
 if !pin.is_empty()&&(pin.len()<4||pin.len()>8||!pin.chars().all(|character|character.is_ascii_digit())){return Err("رمز المالية يجب أن يكون من 4 إلى 8 أرقام إنجليزية".into())}
 if !entry_pin.is_empty()&&(entry_pin.len()<4||entry_pin.len()>8||!entry_pin.chars().all(|character|character.is_ascii_digit())){return Err("رمز دخول التطبيق يجب أن يكون من 4 إلى 8 أرقام إنجليزية".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('shop_name',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[shop_name.trim()]).map_err(|e|e.to_string())?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('owner_name',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[owner_name.trim()]).map_err(|e|e.to_string())?;
 if !pin.is_empty(){conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('finance_pin',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[pin]).map_err(|e|e.to_string())?;}
 if !entry_pin.is_empty(){conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('app_pin',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[entry_pin]).map_err(|e|e.to_string())?;}
 get_app_settings(app)
}

#[tauri::command]
fn clear_finance_pin(app:AppHandle)->Result<AppSettings,String>{
 let conn=db(&app)?;conn.execute("DELETE FROM app_settings WHERE setting_key='finance_pin'",[]).map_err(|e|e.to_string())?;get_app_settings(app)
}

#[tauri::command]
fn clear_app_pin(app:AppHandle)->Result<AppSettings,String>{
 let conn=db(&app)?;conn.execute("DELETE FROM app_settings WHERE setting_key='app_pin'",[]).map_err(|e|e.to_string())?;get_app_settings(app)
}

#[tauri::command]
fn verify_finance_pin(app:AppHandle,pin:String)->Result<bool,String>{
 let conn=db(&app)?;
 let saved:String=conn.query_row("SELECT setting_value FROM app_settings WHERE setting_key='finance_pin'",[],|row|row.get(0)).unwrap_or_default();
 Ok(saved.is_empty()||saved==pin.trim())
}

#[tauri::command]
fn verify_app_pin(app:AppHandle,pin:String)->Result<bool,String>{
 let conn=db(&app)?;
 let saved:String=conn.query_row("SELECT setting_value FROM app_settings WHERE setting_key='app_pin'",[],|row|row.get(0)).unwrap_or_default();
 Ok(saved.is_empty()||saved==pin.trim())
}

#[tauri::command]
fn save_theme(app:AppHandle,theme:String)->Result<AppSettings,String>{
 let value=if theme=="light"{"light"}else{"dark"};
 let conn=db(&app)?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('theme',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[value]).map_err(|e|e.to_string())?;
 get_app_settings(app)
}

#[tauri::command]
fn list_extra_transactions(app:AppHandle)->Result<Vec<ExtraTransaction>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT e.id,e.transaction_type,e.customer_name,e.customer_phone,e.fabric_id,
          COALESCE(f.name,''),COALESCE(f.color,''),e.quantity,e.meters,e.description,e.total_price,e.payment_method,e.created_at
   FROM extra_transactions e LEFT JOIN fabrics f ON f.id=e.fabric_id ORDER BY e.id DESC LIMIT 100"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ExtraTransaction{
  id:row.get(0)?,transaction_type:row.get(1)?,customer_name:row.get(2)?,customer_phone:row.get(3)?,
  fabric_id:row.get(4)?,fabric_name:row.get(5)?,fabric_color:row.get(6)?,quantity:row.get(7)?,meters:row.get(8)?,
  description:row.get(9)?,total_price:row.get(10)?,payment_method:row.get(11)?,created_at:row.get(12)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn save_extra_transaction(app:AppHandle,payload:ExtraTransactionPayload)->Result<ExtraTransaction,String>{
 let transaction_type=payload.transaction_type.trim();
 if transaction_type!="تصليح"&&transaction_type!="بيع قماش"{return Err("نوع العملية غير صحيح".into())}
 if payload.customer_name.trim().is_empty(){return Err("اسم الزبون مطلوب".into())}
 if transaction_type=="تصليح"&&payload.quantity<1{return Err("عدد الثياب المطلوب تصليحها غير صحيح".into())}
 if !payload.total_price.is_finite()||payload.total_price<0.0{return Err("السعر غير صحيح".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 if transaction_type=="بيع قماش"{
  let fabric_id=payload.fabric_id.ok_or_else(||"اختر القماش المباع".to_string())?;
  if !payload.meters.is_finite()||payload.meters<=0.0{return Err("أدخل عدد الأمتار المباعة".into())}
  let stock:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|_|"القماش غير موجود".to_string())?;
  if payload.meters>stock+0.0001{return Err(format!("المتوفر {:.2} متر فقط",stock))}
 }
 transaction.execute(
  "INSERT INTO extra_transactions(transaction_type,customer_name,customer_phone,fabric_id,quantity,meters,description,total_price,payment_method,created_at)
   VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,datetime('now','localtime'))",
  params![transaction_type,payload.customer_name.trim(),payload.customer_phone.trim(),payload.fabric_id,payload.quantity.max(1),payload.meters.max(0.0),payload.description.trim(),payload.total_price,&payload.payment_method]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 if transaction_type=="بيع قماش"{
  let fabric_id=payload.fabric_id.unwrap();
  let stock:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|e|e.to_string())?;
  let balance=stock-payload.meters;
  transaction.execute("UPDATE fabrics SET stock_meters=?1 WHERE id=?2",params![balance,fabric_id]).map_err(|e|e.to_string())?;
  transaction.execute(
   "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
    VALUES(?1,'بيع قماش',?2,?3,'دخل إضافي',?4,?5,datetime('now','localtime'))",
   params![fabric_id,-payload.meters,balance,id,payload.customer_name.trim()]
  ).map_err(|e|e.to_string())?;
 }
 let result=transaction.query_row(
  "SELECT e.id,e.transaction_type,e.customer_name,e.customer_phone,e.fabric_id,
          COALESCE(f.name,''),COALESCE(f.color,''),e.quantity,e.meters,e.description,e.total_price,e.payment_method,e.created_at
   FROM extra_transactions e LEFT JOIN fabrics f ON f.id=e.fabric_id WHERE e.id=?1",
  [id],|row|Ok(ExtraTransaction{
   id:row.get(0)?,transaction_type:row.get(1)?,customer_name:row.get(2)?,customer_phone:row.get(3)?,
   fabric_id:row.get(4)?,fabric_name:row.get(5)?,fabric_color:row.get(6)?,quantity:row.get(7)?,meters:row.get(8)?,
   description:row.get(9)?,total_price:row.get(10)?,payment_method:row.get(11)?,created_at:row.get(12)?,
  })
 ).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(result)
}

fn apply_invoice_fabric_usage(transaction:&rusqlite::Transaction<'_>,invoice_id:i64,usages:&[FabricUsagePayload],confirm_low_stock:bool)->Result<(),String>{
 let existing={
  let mut statement=transaction.prepare("SELECT fabric_id,thobe_index,meters FROM invoice_fabric_usage WHERE invoice_id=?1").map_err(|e|e.to_string())?;
  let rows=statement.query_map([invoice_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,f64>(2)?))).map_err(|e|e.to_string())?;
  rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?
 };
 for (fabric_id,thobe_index,meters) in existing{
  transaction.execute("UPDATE fabrics SET stock_meters=stock_meters+?1 WHERE id=?2",params![meters,fabric_id]).map_err(|e|e.to_string())?;
  let balance:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|e|e.to_string())?;
  transaction.execute(
   "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
    VALUES(?1,'إرجاع تعديل فاتورة',?2,?3,'فاتورة ثوب',?4,?5,datetime('now','localtime'))",
   params![fabric_id,meters,balance,invoice_id,format!("الثوب {}",thobe_index+1)]
  ).map_err(|e|e.to_string())?;
 }
 transaction.execute("DELETE FROM invoice_fabric_usage WHERE invoice_id=?1",[invoice_id]).map_err(|e|e.to_string())?;
 let mut totals=std::collections::BTreeMap::<i64,f64>::new();
 for usage in usages{
  if usage.thobe_index<0||!usage.meters.is_finite()||usage.meters<=0.0{return Err("استهلاك القماش غير صحيح".into())}
  *totals.entry(usage.fabric_id).or_insert(0.0)+=usage.meters;
 }
 for (fabric_id,required) in totals{
  let (stock,name,color):(f64,String,String)=transaction.query_row(
   "SELECT stock_meters,name,color FROM fabrics WHERE id=?1",[fabric_id],
   |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
  ).map_err(|_|"أحد الأقمشة المختارة لم يعد موجودًا".to_string())?;
  if required>stock+0.0001{return Err(format!("قماش {} — {}: المطلوب {:.2} متر والمتوفر {:.2} متر فقط",name,color,required,stock))}
  let remaining=stock-required;
  if remaining<2.5&&!confirm_low_stock{
   return Err(format!("سيبقى من قماش {} — {} مقدار {:.2} متر فقط. فعّل تأكيد المخزون المنخفض ثم احفظ",name,color,remaining.max(0.0)))
  }
 }
 for usage in usages{
  let stock:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[usage.fabric_id],|row|row.get(0)).map_err(|e|e.to_string())?;
  let balance=stock-usage.meters;
  transaction.execute("UPDATE fabrics SET stock_meters=?1 WHERE id=?2",params![balance,usage.fabric_id]).map_err(|e|e.to_string())?;
  transaction.execute(
   "INSERT INTO invoice_fabric_usage(invoice_id,fabric_id,thobe_index,meters) VALUES(?1,?2,?3,?4)",
   params![invoice_id,usage.fabric_id,usage.thobe_index,usage.meters]
  ).map_err(|e|e.to_string())?;
  transaction.execute(
   "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
    VALUES(?1,'تفصيل ثوب',?2,?3,'فاتورة ثوب',?4,?5,datetime('now','localtime'))",
   params![usage.fabric_id,-usage.meters,balance,invoice_id,format!("الثوب {}",usage.thobe_index+1)]
  ).map_err(|e|e.to_string())?;
 }
 Ok(())
}

#[tauri::command]
fn save_invoice(app:AppHandle,payload:InvoicePayload)->Result<InvoiceRecord,String>{
 let mut conn=db(&app)?;
 if let Some(id)=payload.id{
  let transaction=conn.transaction().map_err(|e|e.to_string())?;
  let old_paid_text:String=transaction.query_row("SELECT paid_amount FROM invoices WHERE id=?1 AND customer_id=?2",params![id,payload.customer_id],|row|row.get(0)).map_err(|_|"الفاتورة غير موجودة".to_string())?;
  let payment_delta=parse_money(&payload.paid_amount)-parse_money(&old_paid_text);
  transaction.execute(
   "UPDATE invoices SET weight=?1,delivery_date=?2,day_count=?3,total_thobes=?4,total_price=?5,paid_amount=?6,payment_method=?7,discount=?8,notes=?9,measurements_json=?10,fabric_json=?11,designs_json=?12,details_json=?13,updated_at=datetime('now','localtime') WHERE id=?14 AND customer_id=?15",
   params![&payload.weight,&payload.delivery_date,payload.day_count,payload.total_thobes,&payload.total_price,&payload.paid_amount,&payload.payment_method,&payload.discount,&payload.notes,&payload.measurements_json,&payload.fabric_json,&payload.designs_json,&payload.details_json,id,payload.customer_id]
  ).map_err(|e|e.to_string())?;
  let (invoice_number,created_at,order_id):(String,String,i64)=transaction.query_row(
   "SELECT invoice_number,created_at,order_id FROM invoices WHERE id=?1 AND customer_id=?2",
   params![id,payload.customer_id],
   |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
  ).map_err(|e|e.to_string())?;
  if payment_delta.abs()>0.0001{
   transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![id,format!("تعديل دفعة الفاتورة {}",invoice_number),payment_delta,&payload.payment_method]).map_err(|e|e.to_string())?;
  }
  transaction.execute("UPDATE orders SET quantity=?1,delivery_date=NULLIF(?2,'') WHERE id=?3",params![payload.total_thobes,&payload.delivery_date,order_id]).map_err(|e|e.to_string())?;
  apply_invoice_fabric_usage(&transaction,id,&payload.fabric_usages,payload.confirm_low_stock)?;
  transaction.commit().map_err(|e|e.to_string())?;
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
 let initial_payment=parse_money(&payload.paid_amount);
 if initial_payment>0.0{
  transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![id,format!("دفعة أولية للفاتورة {}",invoice_number),initial_payment,&payload.payment_method]).map_err(|e|e.to_string())?;
 }
 apply_invoice_fabric_usage(&transaction,id,&payload.fabric_usages,payload.confirm_low_stock)?;
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
  .invoke_handler(tauri::generate_handler![dashboard_summary,work_board,advance_order_status,retreat_order_status,current_session,session_history,storage_info,set_storage_location,create_customer,search_customers,customer_invoices,list_design_options,add_design_option,delete_design_option,list_suppliers,add_supplier,list_supplier_payments,add_supplier_payment,list_fabrics,add_fabric,restock_fabric,fabric_movements,list_notes,add_note,toggle_note,delete_note,list_whatsapp_campaigns,save_whatsapp_campaign,financial_overview,add_financial_entry,record_invoice_payment,daily_report,get_app_settings,save_app_settings,clear_finance_pin,clear_app_pin,verify_finance_pin,verify_app_pin,save_theme,list_extra_transactions,save_extra_transaction,save_invoice])
  .run(tauri::generate_context!())
  .expect("تعذر تشغيل TAILOR");
}
