#![cfg_attr(all(windows, not(debug_assertions), not(test)), windows_subsystem = "windows")]

use rusqlite::{params,Connection,OptionalExtension};
use serde::{Deserialize,Serialize};
use std::{fs,path::{Path,PathBuf},sync::Mutex};
use tauri::{AppHandle,Manager,WindowEvent};
mod license;
use license::{activate_license,deactivate_license,license_status};

#[cfg(windows)]
#[tauri::command]
fn list_printers()->Result<Vec<String>,String>{
 use windows::{core::PCWSTR,Win32::Graphics::Printing::{EnumPrintersW,PRINTER_ENUM_LOCAL,PRINTER_ENUM_CONNECTIONS,PRINTER_INFO_4W}};
 let flags=PRINTER_ENUM_LOCAL|PRINTER_ENUM_CONNECTIONS;
 let (mut needed,mut count)=(0,0);
 let _=unsafe{EnumPrintersW(flags,PCWSTR::null(),4,None,&mut needed,&mut count)};
 if needed==0{return Ok(Vec::new())}
 if needed>1024*1024{return Err("قائمة الطابعات كبيرة جدًا".into())}
 let mut buffer=vec![0u8;needed as usize];
 unsafe{EnumPrintersW(flags,PCWSTR::null(),4,Some(&mut buffer),&mut needed,&mut count)}.map_err(|e|e.to_string())?;
 let mut names=Vec::new();
 for index in 0..count as usize{
  let offset=index*std::mem::size_of::<PRINTER_INFO_4W>();
  if offset+std::mem::size_of::<PRINTER_INFO_4W>()>buffer.len(){break}
  let info=unsafe{(buffer.as_ptr().add(offset) as *const PRINTER_INFO_4W).read_unaligned()};
  let name=unsafe{info.pPrinterName.to_string()}.map_err(|e|e.to_string())?;
  if !name.is_empty(){names.push(name)}
 }
 names.sort();names.dedup();Ok(names)
}

#[cfg(not(windows))]
#[tauri::command]
fn list_printers()->Result<Vec<String>,String>{Ok(Vec::new())}

// The spooler's status is the best available readiness signal; some drivers
// report a disconnected USB printer as idle until a print job is submitted.
#[cfg(windows)]
#[tauri::command]
fn report_printer_ready(printer_name:String)->Result<bool,String>{
 use windows::{core::{PCWSTR,PWSTR},Win32::Graphics::Printing::{GetDefaultPrinterW,OpenPrinterW,GetPrinterW,ClosePrinter,PRINTER_HANDLE,PRINTER_INFO_2W,PRINTER_STATUS_OFFLINE,PRINTER_STATUS_NOT_AVAILABLE,PRINTER_STATUS_ERROR,PRINTER_STATUS_PAUSED,PRINTER_STATUS_PAPER_OUT,PRINTER_STATUS_PAPER_JAM,PRINTER_STATUS_NO_TONER,PRINTER_STATUS_DOOR_OPEN,PRINTER_ATTRIBUTE_WORK_OFFLINE}};
 let name=if printer_name.trim().is_empty(){
  let mut length=0u32;
  unsafe{GetDefaultPrinterW(None,&mut length)};
  if length==0{return Ok(false)}
  let mut buffer=vec![0u16;length as usize];
  if !unsafe{GetDefaultPrinterW(Some(PWSTR(buffer.as_mut_ptr())),&mut length)}.as_bool(){return Ok(false)}
  String::from_utf16_lossy(&buffer[..buffer.iter().position(|unit|*unit==0).unwrap_or(buffer.len())])
 }else{printer_name};
 let wide:Vec<u16>=name.encode_utf16().chain(std::iter::once(0)).collect();
 let mut printer=PRINTER_HANDLE::default();
 if unsafe{OpenPrinterW(PCWSTR(wide.as_ptr()),&mut printer,None)}.is_err(){return Ok(false)}
 let result=(||->Result<bool,String>{
  let mut needed=0u32;
  let _=unsafe{GetPrinterW(printer,2,None,&mut needed)};
  if needed==0||needed>1024*1024{return Ok(false)}
  let mut buffer=vec![0u8;needed as usize];
  unsafe{GetPrinterW(printer,2,Some(&mut buffer),&mut needed)}.map_err(|e|e.to_string())?;
  if buffer.len()<std::mem::size_of::<PRINTER_INFO_2W>(){return Ok(false)}
  let info=unsafe{(buffer.as_ptr() as *const PRINTER_INFO_2W).read_unaligned()};
  let blocked=PRINTER_STATUS_OFFLINE|PRINTER_STATUS_NOT_AVAILABLE|PRINTER_STATUS_ERROR|PRINTER_STATUS_PAUSED|PRINTER_STATUS_PAPER_OUT|PRINTER_STATUS_PAPER_JAM|PRINTER_STATUS_NO_TONER|PRINTER_STATUS_DOOR_OPEN;
  Ok(info.Status&blocked==0&&info.Attributes&PRINTER_ATTRIBUTE_WORK_OFFLINE==0)
 })();
 let _=unsafe{ClosePrinter(printer)};
 result
}

#[cfg(not(windows))]
#[tauri::command]
fn report_printer_ready(_printer_name:String)->Result<bool,String>{Ok(false)}

// WebView2 prints the current page directly to the Windows default printer.
// The frontend selects exactly one document with print CSS before invoking this command.
#[cfg(windows)]
#[tauri::command]
async fn print_direct(window:tauri::WebviewWindow,thermal:bool,page_height_mm:f64,printer_name:String)->Result<(),String>{
 use std::{sync::mpsc,time::Duration};
 use webview2_com::{Microsoft::Web::WebView2::Win32::*,PrintCompletedHandler};
 use windows::core::Interface;
 if !page_height_mm.is_finite()||!(70.0..=1500.0).contains(&page_height_mm){return Err("مقاس الورق غير صالح".into())}
 let (tx,rx)=mpsc::channel::<Result<(),String>>();
 window.with_webview(move |webview|{
  let result=(||->Result<(),String>{
   let core=unsafe{webview.controller().CoreWebView2()}.map_err(|e|e.to_string())?;
   // Wry and this print module may use different windows-core versions.
   // Both COM wrappers own one pointer; moving it to IUnknown transfers that
   // ownership so QueryInterface uses the same windows-core as webview2-com.
   let core:windows::core::IUnknown=unsafe{std::mem::transmute(core)};
   let printer:ICoreWebView2_16=core.cast().map_err(|e|format!("واجهة الطباعة غير متاحة: {e}"))?;
   let environment:windows::core::IUnknown=unsafe{std::mem::transmute(webview.environment())};
   let environment:ICoreWebView2Environment6=environment.cast().map_err(|e|e.to_string())?;
   let settings=unsafe{environment.CreatePrintSettings()}.map_err(|e|e.to_string())?;
   let media:ICoreWebView2PrintSettings2=settings.cast().map_err(|e|format!("إعداد حجم الورق غير متاح: {e}"))?;
   unsafe{
    media.SetMediaSize(COREWEBVIEW2_PRINT_MEDIA_SIZE_CUSTOM).map_err(|e|e.to_string())?;
    settings.SetPageWidth(if thermal{80.0/25.4}else{210.0/25.4}).map_err(|e|e.to_string())?;
    settings.SetPageHeight(page_height_mm/25.4).map_err(|e|e.to_string())?;
    settings.SetMarginTop(0.0).map_err(|e|e.to_string())?;
    settings.SetMarginBottom(0.0).map_err(|e|e.to_string())?;
    settings.SetMarginLeft(0.0).map_err(|e|e.to_string())?;
    settings.SetMarginRight(0.0).map_err(|e|e.to_string())?;
    settings.SetShouldPrintHeaderAndFooter(false).map_err(|e|e.to_string())?;
    settings.SetScaleFactor(1.0).map_err(|e|e.to_string())?;
    if !printer_name.is_empty(){
     let wide:Vec<u16>=printer_name.encode_utf16().chain(std::iter::once(0)).collect();
     media.SetPrinterName(windows::core::PCWSTR::from_raw(wide.as_ptr())).map_err(|e|e.to_string())?;
    }
   }
   let completed_tx=tx.clone();
   let completed=PrintCompletedHandler::create(Box::new(move |operation,status|{
    let result=match operation{
     Err(error)=>Err(error.to_string()),
     Ok(()) if status==COREWEBVIEW2_PRINT_STATUS_SUCCEEDED=>Ok(()),
     Ok(()) if status==COREWEBVIEW2_PRINT_STATUS_PRINTER_UNAVAILABLE=>Err("الطابعة الافتراضية غير متاحة أو غير متصلة".into()),
     Ok(())=>Err("فشلت الطباعة على الطابعة الافتراضية".into()),
    };
    let _=completed_tx.send(result);
    Ok(())
   }));
   unsafe{printer.Print(&settings,&completed)}.map_err(|e|e.to_string())?;
   Ok(())
  })();
  if let Err(error)=result{let _=tx.send(Err(error));}
 }).map_err(|e|e.to_string())?;
 tauri::async_runtime::spawn_blocking(move ||rx.recv_timeout(Duration::from_secs(60)))
  .await.map_err(|e|e.to_string())?.map_err(|_|"انتهت مهلة انتظار الطابعة".to_string())?
}

#[cfg(not(windows))]
#[tauri::command]
fn print_direct(_window:tauri::WebviewWindow,_thermal:bool,_page_height_mm:f64,_printer_name:String)->Result<(),String>{
 Err("الطباعة المباشرة متاحة على Windows فقط".into())
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Dashboard{received_today:i64,tailored_today:i64,ready_to_deliver:i64}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkBoardItem{
 order_id:i64,invoice_number:String,customer_name:String,customer_code:String,phone:String,delivery_date:String,quantity:i64,status:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DeliveryInvoiceBalance{invoice_id:i64,invoice_number:String,remaining:f64}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct DeliveryContext{
 order_id:i64,quantity:i64,customer_name:String,current:DeliveryInvoiceBalance,older:Vec<DeliveryInvoiceBalance>,total_debt:f64
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
struct Supplier{id:i64,name:String,phone:String,notes:String,fabric_count:i64,total_purchases:f64,total_paid:f64,balance:f64,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SupplierPayment{id:i64,supplier_id:i64,supplier_name:String,amount:f64,payment_method:String,notes:String,created_at:String}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct SupplierLedgerEntry{
 source_type:String,source_id:i64,supplier_id:i64,entry_type:String,title:String,details:String,amount:f64,created_at:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct FabricItem{
 id:i64,supplier_id:Option<i64>,supplier_name:String,name:String,color:String,
 stock_meters:f64,purchase_price:f64,sale_price:f64,created_at:String,
 kind:String,catalog_number:String,color_number:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ReadyProduct{
 id:i64,name:String,size:String,fabric_id:i64,fabric_label:String,stock_quantity:i64,sale_price:f64,created_at:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ReadyProduction{
 id:i64,product_id:i64,product_name:String,size:String,quantity:i64,fabric_meters:f64,tailor_name:String,created_at:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ReadyItem{
 id:i64,quantity:i64,initial_quantity:i64,length_inches:f64,width_inches:f64,
 unit:String,measurements_json:String,designs_json:String,details_json:String,
 thobe_type:String,fabric_id:Option<i64>,fabric_label:String,sale_price:f64,created_at:String
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct ReadyItemPayload{
 quantity:i64,unit:String,measurements_json:String,designs_json:String,details_json:String,
 thobe_type:String,fabric_id:Option<i64>,fabric_meters:f64,sale_price:f64
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
 report_date:String,end_date:String,period:String,period_label:String,new_customers:i64,invoices:i64,thobes:i64,invoice_sales:f64,invoice_received:f64,
 extra_income:f64,expenses:f64,delivered:i64,fabric_used:f64,fabric_sold:f64
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct AppSettings{shop_name:String,owner_name:String,accountant_name:String,finance_pin_set:bool,app_pin_set:bool,theme:String,initialized:bool,large_cut_price:f64,small_cut_price:f64,auto_report_enabled:bool,auto_report_time:String,printer_check_minutes:i64}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct FabricUsagePayload{fabric_id:i64,thobe_index:i64,meters:f64}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct WorkerCutPayload{thobe_index:i64,worker_name:String,tailor_name:Option<String>,thobe_size:String,amount:f64,#[serde(default)] tailor_amount:Option<f64>}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct PaymentSplit{method:String,amount:f64}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct DeliveryPaymentAllocation{invoice_id:i64,payment_splits:Vec<PaymentSplit>}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkerLedgerEntry{
 id:i64,invoice_id:i64,invoice_number:String,customer_name:String,thobe_index:i64,
 thobe_size:String,amount:f64,created_at:String,worker_name:String
}


#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkerProfile{
 id:i64,name:String,role:String,pay_mode:String,monthly_salary:f64,large_rate:f64,small_rate:f64,
 salary_start:String,active:bool,archived_at:String,created_at:String
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkerMovement{
 id:i64,entry_type:String,description:String,amount:f64,created_at:String,
 invoice_number:String,customer_name:String,thobe_index:i64,thobe_size:String,payment_method:String
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkerSalaryChange{effective_month:String,monthly_salary:f64}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct WorkerAccount{worker:WorkerProfile,movements:Vec<WorkerMovement>,salary_changes:Vec<WorkerSalaryChange>}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct InvoicePayload{
 id:Option<i64>,customer_id:i64,weight:String,delivery_date:String,day_count:i64,total_thobes:i64,
 total_price:String,paid_amount:String,payment_method:String,discount:String,notes:String,
 #[serde(default)] new_customer:Option<NewCustomerPayload>,
 #[serde(default)] minimum_price:Option<f64>,
 #[serde(default)] payment_splits:Option<Vec<PaymentSplit>>,
 measurements_json:String,fabric_json:String,designs_json:String,details_json:String,
 fabric_usages:Vec<FabricUsagePayload>,worker_cuts:Vec<WorkerCutPayload>,confirm_low_stock:bool
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct InvoiceRecord{id:i64,customer_id:i64,invoice_number:String,created_at:String,payment_method:String}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct NewCustomerPayload{name:String,phone:String}

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
 quantity:i64,meters:f64,description:String,total_price:f64,payment_method:String,worker_name:String,
 ready_product_id:Option<i64>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ExtraTransaction{
 id:i64,transaction_type:String,customer_name:String,customer_phone:String,fabric_id:Option<i64>,
 fabric_name:String,fabric_color:String,quantity:i64,meters:f64,description:String,total_price:f64,
 payment_method:String,worker_name:String,created_at:String,ready_product_name:String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct StorageInfo{folder:String,database_path:String,is_custom:bool}

pub(crate) struct SessionState(pub(crate) Mutex<Option<i64>>);

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

fn migrate_fabric_classification(conn:&Connection)->Result<(),String>{
 if !has_column(conn,"fabrics","kind")?{conn.execute("ALTER TABLE fabrics ADD COLUMN kind TEXT NOT NULL DEFAULT 'أبيض'",[]).map_err(|e|e.to_string())?;}
 if !has_column(conn,"fabrics","catalog_number")?{conn.execute("ALTER TABLE fabrics ADD COLUMN catalog_number TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 if !has_column(conn,"fabrics","color_number")?{conn.execute("ALTER TABLE fabrics ADD COLUMN color_number TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 Ok(())
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
  CREATE TABLE IF NOT EXISTS workers(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   name TEXT NOT NULL UNIQUE,
   created_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS worker_cut_entries(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   invoice_id INTEGER NOT NULL,
   thobe_index INTEGER NOT NULL,
   worker_name TEXT NOT NULL,
   thobe_size TEXT NOT NULL,
   amount REAL NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL,
   UNIQUE(invoice_id,thobe_index),
   FOREIGN KEY(invoice_id) REFERENCES invoices(id)
  );
  CREATE TABLE IF NOT EXISTS worker_tailor_entries(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   invoice_id INTEGER NOT NULL,
   thobe_index INTEGER NOT NULL,
   worker_id INTEGER NOT NULL,
   thobe_size TEXT NOT NULL,
   amount REAL NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL,
   UNIQUE(invoice_id,thobe_index),
   FOREIGN KEY(invoice_id) REFERENCES invoices(id),
   FOREIGN KEY(worker_id) REFERENCES workers(id)
  );
  CREATE TABLE IF NOT EXISTS worker_withdrawals(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   worker_id INTEGER NOT NULL,
   kind TEXT NOT NULL,
   amount REAL NOT NULL,
   details TEXT NOT NULL DEFAULT '',
   payment_method TEXT NOT NULL DEFAULT '',
   created_at TEXT NOT NULL,
   FOREIGN KEY(worker_id) REFERENCES workers(id)
  );
  CREATE TABLE IF NOT EXISTS worker_salary_changes(
   worker_id INTEGER NOT NULL,
   effective_month TEXT NOT NULL,
   monthly_salary REAL NOT NULL,
   PRIMARY KEY(worker_id,effective_month),
   FOREIGN KEY(worker_id) REFERENCES workers(id)
  );
  CREATE TABLE IF NOT EXISTS app_settings(
   setting_key TEXT PRIMARY KEY,
   setting_value TEXT NOT NULL DEFAULT ''
  );
  CREATE TABLE IF NOT EXISTS ready_products(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   name TEXT NOT NULL,
   size TEXT NOT NULL,
   fabric_id INTEGER NOT NULL,
   stock_quantity INTEGER NOT NULL DEFAULT 0,
   initial_quantity INTEGER NOT NULL DEFAULT 0,
   length_inches REAL NOT NULL DEFAULT 0,
   width_inches REAL NOT NULL DEFAULT 0,
   unit TEXT NOT NULL DEFAULT 'إنش',
   measurements_json TEXT NOT NULL DEFAULT '{}',
   designs_json TEXT NOT NULL DEFAULT '{}',
   details_json TEXT NOT NULL DEFAULT '{}',
   thobe_type TEXT NOT NULL DEFAULT 'سعودي',
   sale_price REAL NOT NULL DEFAULT 0,
   created_at TEXT NOT NULL,
   FOREIGN KEY(fabric_id) REFERENCES fabrics(id)
  );
  CREATE TABLE IF NOT EXISTS ready_productions(
   id INTEGER PRIMARY KEY AUTOINCREMENT,
   product_id INTEGER NOT NULL,
   quantity INTEGER NOT NULL,
   fabric_meters REAL NOT NULL,
   tailor_name TEXT NOT NULL DEFAULT '',
   created_at TEXT NOT NULL,
   FOREIGN KEY(product_id) REFERENCES ready_products(id)
  );
 ").map_err(|e|e.to_string())?;
 for (column,declaration) in [
  ("initial_quantity","INTEGER NOT NULL DEFAULT 0"),("length_inches","REAL NOT NULL DEFAULT 0"),
  ("width_inches","REAL NOT NULL DEFAULT 0"),("unit","TEXT NOT NULL DEFAULT 'إنش'"),
  ("measurements_json","TEXT NOT NULL DEFAULT '{}'"),("designs_json","TEXT NOT NULL DEFAULT '{}'"),
  ("details_json","TEXT NOT NULL DEFAULT '{}'"),("thobe_type","TEXT NOT NULL DEFAULT 'سعودي'")
 ]{
  if !has_column(&conn,"ready_products",column)?{
   conn.execute(&format!("ALTER TABLE ready_products ADD COLUMN {column} {declaration}"),[]).map_err(|e|e.to_string())?;
  }
 }
 conn.execute("UPDATE ready_products SET initial_quantity=stock_quantity WHERE initial_quantity=0 AND measurements_json='{}'",[]).map_err(|e|e.to_string())?;
 conn.execute("UPDATE ready_products SET length_inches=CAST(size AS REAL) WHERE length_inches=0 AND trim(size)<>''",[]).map_err(|e|e.to_string())?;
 migrate_fabric_classification(&conn)?;
 if !has_column(&conn,"extra_transactions","ready_product_id")?{conn.execute("ALTER TABLE extra_transactions ADD COLUMN ready_product_id INTEGER",[]).map_err(|e|e.to_string())?;}
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
 conn.execute("CREATE INDEX IF NOT EXISTS idx_orders_status_delivered_at ON orders(work_status,delivered_at)",[]).map_err(|e|e.to_string())?;
 if !has_column(&conn,"fabric_movements","entry_unit")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN entry_unit TEXT NOT NULL DEFAULT 'متر'",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","carton_count")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN carton_count REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","meters_per_carton")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN meters_per_carton REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","total_cost")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN total_cost REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"fabric_movements","unit_cost")?{conn.execute("ALTER TABLE fabric_movements ADD COLUMN unit_cost REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"financial_entries","reference_type")?{conn.execute("ALTER TABLE financial_entries ADD COLUMN reference_type TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"financial_entries","reference_id")?{conn.execute("ALTER TABLE financial_entries ADD COLUMN reference_id INTEGER",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"extra_transactions","worker_name")?{conn.execute("ALTER TABLE extra_transactions ADD COLUMN worker_name TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","role")?{conn.execute("ALTER TABLE workers ADD COLUMN role TEXT NOT NULL DEFAULT 'قصاص'",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","pay_mode")?{conn.execute("ALTER TABLE workers ADD COLUMN pay_mode TEXT NOT NULL DEFAULT 'قطعة'",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","monthly_salary")?{conn.execute("ALTER TABLE workers ADD COLUMN monthly_salary REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","large_rate")?{
  conn.execute("ALTER TABLE workers ADD COLUMN large_rate REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;
  conn.execute("UPDATE workers SET large_rate=COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='large_cut_price'),30)",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"workers","small_rate")?{
  conn.execute("ALTER TABLE workers ADD COLUMN small_rate REAL NOT NULL DEFAULT 0",[]).map_err(|e|e.to_string())?;
  conn.execute("UPDATE workers SET small_rate=COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='small_cut_price'),25)",[]).map_err(|e|e.to_string())?;
 }
 if !has_column(&conn,"workers","salary_start")?{conn.execute("ALTER TABLE workers ADD COLUMN salary_start TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","active")?{conn.execute("ALTER TABLE workers ADD COLUMN active INTEGER NOT NULL DEFAULT 1",[]).map_err(|e|e.to_string())?;}
 if !has_column(&conn,"workers","archived_at")?{conn.execute("ALTER TABLE workers ADD COLUMN archived_at TEXT NOT NULL DEFAULT ''",[]).map_err(|e|e.to_string())?;}
 conn.execute("UPDATE workers SET salary_start=substr(created_at,1,7)||'-01' WHERE salary_start=''",[]).map_err(|e|e.to_string())?;
 conn.execute_batch("
  UPDATE customers SET customer_code='__customer_' || id;
  UPDATE customers SET customer_code=CAST(id AS TEXT);
 UPDATE invoices SET invoice_number='__invoice_' || id;
 UPDATE invoices SET invoice_number=CAST(id AS TEXT);
  UPDATE design_options SET category='الكبك' WHERE category='الكباك';
  INSERT OR IGNORE INTO workers(name,created_at,role,pay_mode,large_rate,small_rate,salary_start)
  SELECT trim(worker_name),MIN(created_at),'قصاص','قطعة',
   COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='large_cut_price'),30),
   COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='small_cut_price'),25),
   substr(MIN(created_at),1,7)||'-01'
  FROM worker_cut_entries
  WHERE trim(worker_name)<>''
  GROUP BY trim(worker_name);
  INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at)
  SELECT 'دفعة فاتورة',i.id,'دفعة أولية للفاتورة ' || i.invoice_number,CAST(NULLIF(i.paid_amount,'') AS REAL),i.payment_method,i.created_at
  FROM invoices i
  WHERE CAST(NULLIF(i.paid_amount,'') AS REAL)>0
    AND NOT EXISTS(SELECT 1 FROM financial_entries f WHERE f.invoice_id=i.id AND f.entry_type='دفعة فاتورة');
  INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at,reference_type,reference_id)
  SELECT e.transaction_type,NULL,e.transaction_type || ' — ' || e.customer_name || ' — فاتورة ' || e.id,e.total_price,e.payment_method,e.created_at,'دخل إضافي',e.id
  FROM extra_transactions e
  WHERE e.total_price>0
    AND NOT EXISTS(
      SELECT 1 FROM financial_entries f
      WHERE f.reference_type='دخل إضافي' AND f.reference_id=e.id
    );
  UPDATE financial_entries
  SET reference_type='دفعة مورد',
      reference_id=(
       SELECT p.id
       FROM supplier_payments p
       JOIN suppliers s ON s.id=p.supplier_id
       WHERE p.amount=financial_entries.amount
         AND p.created_at=financial_entries.created_at
         AND financial_entries.description='دفعة للمورد ' || s.name
       ORDER BY p.id DESC LIMIT 1
      )
  WHERE entry_type='دفعة مورد'
    AND COALESCE(reference_type,'')=''
    AND EXISTS(
      SELECT 1
      FROM supplier_payments p
      JOIN suppliers s ON s.id=p.supplier_id
      WHERE p.amount=financial_entries.amount
        AND p.created_at=financial_entries.created_at
        AND financial_entries.description='دفعة للمورد ' || s.name
    );
 ").map_err(|e|e.to_string())?;
 Ok(conn)
}

pub(crate) fn begin_session(app:&AppHandle)->Result<i64,String>{
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
 dashboard_from_connection(&conn)
}

fn dashboard_from_connection(conn:&Connection)->Result<Dashboard,String>{
 let received_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE received_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let tailored_today=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE tailored_date=date('now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 let ready_to_deliver=conn.query_row("SELECT COALESCE(SUM(quantity),0) FROM orders WHERE work_status='في المحل بانتظار التسليم'",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 Ok(Dashboard{received_today,tailored_today,ready_to_deliver})
}

#[tauri::command]
fn work_board(app:AppHandle)->Result<Vec<WorkBoardItem>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT o.id,i.invoice_number,c.name,c.customer_code,c.phone,i.delivery_date,o.quantity,o.work_status
   FROM orders o
   JOIN invoices i ON i.order_id=o.id
   JOIN customers c ON c.id=i.customer_id
   WHERE o.work_status<>'تم التسليم'
      OR (o.delivered_at>=datetime('now','localtime','start of day')
          AND o.delivered_at<datetime('now','localtime','start of day','+1 day'))
   ORDER BY o.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(WorkBoardItem{
  order_id:row.get(0)?,invoice_number:row.get(1)?,customer_name:row.get(2)?,
  customer_code:row.get(3)?,phone:row.get(4)?,delivery_date:row.get(5)?,quantity:row.get(6)?,status:row.get(7)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

fn delivery_context_from_connection(conn:&Connection,order_id:i64)->Result<DeliveryContext,String>{
 let (customer_id,customer_name,invoice_id,invoice_number,remaining,quantity):(i64,String,i64,String,f64,i64)=conn.query_row(
  &format!("SELECT i.customer_id,c.name,i.id,i.invoice_number,{INVOICE_REMAINING_EXPRESSION},o.quantity FROM orders o JOIN invoices i ON i.order_id=o.id JOIN customers c ON c.id=i.customer_id WHERE o.id=?1 AND o.work_status='في المحل بانتظار التسليم'"),
  [order_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))
 ).map_err(|_|"الطلب غير موجود في مرحلة انتظار التسليم".to_string())?;
 let mut statement=conn.prepare(&format!("SELECT i.id,i.invoice_number,{INVOICE_REMAINING_EXPRESSION} FROM invoices i WHERE i.customer_id=?1 AND i.id<>?2 AND {INVOICE_REMAINING_EXPRESSION}>0.0001 ORDER BY i.id DESC")).map_err(|e|e.to_string())?;
 let older=statement.query_map(params![customer_id,invoice_id],|row|Ok(DeliveryInvoiceBalance{invoice_id:row.get(0)?,invoice_number:row.get(1)?,remaining:row.get(2)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 let total_debt=remaining+older.iter().map(|item:&DeliveryInvoiceBalance|item.remaining).sum::<f64>();
 Ok(DeliveryContext{order_id,quantity,customer_name,current:DeliveryInvoiceBalance{invoice_id,invoice_number,remaining},older,total_debt})
}

#[tauri::command]
fn delivery_payment_context(app:AppHandle,order_id:i64)->Result<DeliveryContext,String>{
 delivery_context_from_connection(&db(&app)?,order_id)
}

fn update_order_stage(conn:&Connection,order_id:i64,status:&str)->Result<(),String>{
 conn.execute(
  "UPDATE orders SET work_status=?1,
   tailored_date=CASE WHEN ?1 IN ('انتظار القص','عند الخياط') THEN NULL WHEN tailored_date IS NULL THEN date('now','localtime') ELSE tailored_date END,
   delivered_at=CASE WHEN ?1='تم التسليم' THEN COALESCE(delivered_at,datetime('now','localtime')) ELSE NULL END
   WHERE id=?2",params![status,order_id]
 ).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn advance_order_status(app:AppHandle,order_id:i64)->Result<(),String>{
 let conn=db(&app)?;
 let current:String=conn.query_row("SELECT work_status FROM orders WHERE id=?1",[order_id],|row|row.get(0)).map_err(|_|"تعذر العثور على طلب الثوب".to_string())?;
 let next=match current.as_str(){
  "انتظار القص"=>"عند الخياط",
  "عند الخياط"=>"في المغسلة",
  "في المغسلة"=>"في المحل بانتظار التسليم",
  "في المحل بانتظار التسليم"=>return Err("افتح شاشة التسليم وسجل الدفعة أو أكد التسليم دون دفع".into()),
  "تم التسليم"=>return Ok(()),
  _=>return Err("حالة الثوب غير معروفة".into()),
 };
 update_order_stage(&conn,order_id,next)
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
 update_order_stage(&conn,order_id,previous)
}

#[tauri::command]
fn move_orders_to_status(app:AppHandle,order_ids:Vec<i64>,status:String)->Result<(),String>{
 let allowed=["انتظار القص","عند الخياط","في المغسلة","في المحل بانتظار التسليم","تم التسليم"];
 if !allowed.contains(&status.as_str()){return Err("مرحلة العمل غير صحيحة".into())}
 if status=="تم التسليم"{return Err("سلّم كل فاتورة من شاشة التسليم لتسجيل دفعاتها وديون العميل".into())}
 if order_ids.is_empty(){return Err("حدد ثوبًا واحدًا على الأقل".into())}
 let mut conn=db(&app)?;let transaction=conn.transaction().map_err(|e|e.to_string())?;
 for order_id in order_ids.iter(){update_order_stage(&transaction,*order_id,&status)?;}
 transaction.commit().map_err(|e|e.to_string())
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

fn normalized_customer_phone(phone:&str)->String{
 let digits:String=phone.chars().filter_map(|character|match character{
  '0'..='9'=>Some(character),
  '٠'..='٩'=>Some(char::from(b'0'+(character as u32-'٠' as u32) as u8)),
  '۰'..='۹'=>Some(char::from(b'0'+(character as u32-'۰' as u32) as u8)),
  _=>None
 }).collect();
 let international=digits.strip_prefix("00").unwrap_or(&digits);
 if international.len()==12&&international.starts_with("9665"){
  format!("0{}",&international[3..])
 }else{international.to_string()}
}

fn ensure_customer_phone_available(conn:&Connection,phone:&str,except_id:Option<i64>)->Result<(),String>{
 let normalized=normalized_customer_phone(phone);
 let mut statement=conn.prepare("SELECT id,name,phone FROM customers").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?))).map_err(|e|e.to_string())?;
 for row in rows{
  let (id,name,stored_phone)=row.map_err(|e|e.to_string())?;
  if Some(id)!=except_id&&normalized_customer_phone(&stored_phone)==normalized{
   return Err(format!("رقم الجوال موجود مسبقًا للعميل {name}."))
  }
 }
 Ok(())
}

#[cfg(test)]
mod customer_phone_tests{
 use super::{ensure_customer_phone_available,find_customer_by_phone_from_connection,normalized_customer_phone};
 use rusqlite::Connection;

 #[test]
 fn local_and_international_saudi_numbers_match(){
  assert_eq!(normalized_customer_phone("٠٥٠ ١٢٣-٤٥٦٧"),"0501234567");
  assert_eq!(normalized_customer_phone("+966 50 123 4567"),"0501234567");
  assert_eq!(normalized_customer_phone("00966 50 123 4567"),"0501234567");
 }

 #[test]
 fn another_customer_cannot_reuse_phone_but_same_customer_can_keep_it(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,name TEXT,phone TEXT);INSERT INTO customers VALUES(1,'علي','0501234567');").unwrap();
  assert!(ensure_customer_phone_available(&conn,"+966 50 123 4567",None).unwrap_err().contains("موجود مسبقًا"));
  assert!(ensure_customer_phone_available(&conn,"0501234567",Some(2)).is_err());
  assert!(ensure_customer_phone_available(&conn,"0501234567",Some(1)).is_ok());
  assert!(ensure_customer_phone_available(&conn,"0550000000",None).is_ok());
 }

 #[test]
 fn duplicate_phone_lookup_returns_the_existing_record_and_measurement_history_count(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,customer_code TEXT,name TEXT,phone TEXT);
   CREATE TABLE invoices(id INTEGER PRIMARY KEY,customer_id INTEGER,created_at TEXT,total_price TEXT,paid_amount TEXT,discount TEXT);
   INSERT INTO customers VALUES(1,'101','علي','0501234567');
   INSERT INTO invoices VALUES(1,1,'2026-09-30','100','40','0');").unwrap();
  let found=find_customer_by_phone_from_connection(&conn,"+966 50 123 4567").unwrap().unwrap();
  assert_eq!(found.id,1);
  assert_eq!(found.invoice_count,1);
  assert_eq!(found.outstanding,60.0);
  assert!(find_customer_by_phone_from_connection(&conn,"0550000000").unwrap().is_none());
 }
}

#[tauri::command]
fn create_customer(app:AppHandle,name:String,phone:String)->Result<Customer,String>{
 let name=name.trim().to_string();
 let phone=phone.trim().to_string();
 if name.is_empty(){return Err("اسم العميل مطلوب".into())}
 if normalized_customer_phone(&phone).len()<7{
  return Err("رقم الجوال غير صحيح".into())
 }
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 ensure_customer_phone_available(&transaction,&phone,None)?;
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
fn update_customer(app:AppHandle,customer_id:i64,name:String,phone:String)->Result<Customer,String>{
 let name=name.trim().to_string();let phone=phone.trim().to_string();
 if name.is_empty(){return Err("اسم العميل مطلوب".into())}
 if normalized_customer_phone(&phone).len()<7{return Err("رقم الجوال غير صحيح".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let code:String=transaction.query_row("SELECT customer_code FROM customers WHERE id=?1",[customer_id],|row|row.get(0)).map_err(|_|"العميل غير موجود".to_string())?;
 ensure_customer_phone_available(&transaction,&phone,Some(customer_id))?;
 transaction.execute("UPDATE customers SET name=?1,phone=?2 WHERE id=?3",params![&name,&phone,customer_id]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(Customer{id:customer_id,code,name,phone})
}

#[tauri::command]
fn delete_customer(app:AppHandle,customer_id:i64)->Result<(),String>{
 let mut conn=db(&app)?;
 delete_customer_from_connection(&mut conn,customer_id)
}

fn delete_customer_from_connection(conn:&mut Connection,customer_id:i64)->Result<(),String>{
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let ids={
  let mut statement=transaction.prepare("SELECT id FROM invoices WHERE customer_id=?1").map_err(|e|e.to_string())?;
  let rows=statement.query_map([customer_id],|row|row.get::<_,i64>(0)).map_err(|e|e.to_string())?;
  rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?
 };
 for invoice_id in ids{delete_invoice_in_transaction(&transaction,invoice_id,customer_id)?}
 let deleted=transaction.execute("DELETE FROM customers WHERE id=?1",[customer_id]).map_err(|e|e.to_string())?;
 if deleted==0{return Err("العميل غير موجود".into())}
 transaction.commit().map_err(|e|e.to_string())
}

fn delete_invoice_from_connection(conn:&mut Connection,invoice_id:i64,customer_id:i64)->Result<(),String>{
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 delete_invoice_in_transaction(&transaction,invoice_id,customer_id)?;
 transaction.commit().map_err(|e|e.to_string())
}

fn delete_invoice_in_transaction(transaction:&rusqlite::Transaction<'_>,invoice_id:i64,customer_id:i64)->Result<(),String>{
 let (order_id,number):(i64,String)=transaction.query_row(
  "SELECT order_id,invoice_number FROM invoices WHERE id=?1 AND customer_id=?2",
  params![invoice_id,customer_id],|row|Ok((row.get(0)?,row.get(1)?))
 ).map_err(|_|"الفاتورة غير موجودة لهذا العميل".to_string())?;
 {
  let mut statement=transaction.prepare("SELECT fabric_id,meters FROM invoice_fabric_usage WHERE invoice_id=?1").map_err(|e|e.to_string())?;
  let usages=statement.query_map([invoice_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,f64>(1)?))).map_err(|e|e.to_string())?
   .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
  for (fabric_id,meters) in usages{
   transaction.execute("UPDATE fabrics SET stock_meters=stock_meters+?1 WHERE id=?2",params![meters,fabric_id]).map_err(|e|e.to_string())?;
   let balance:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|e|e.to_string())?;
   transaction.execute(
    "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
     VALUES(?1,'إلغاء فاتورة',?2,?3,'إلغاء فاتورة',?4,?5,datetime('now','localtime'))",
    params![fabric_id,meters,balance,invoice_id,format!("استرجاع قماش الفاتورة {number}")]
   ).map_err(|e|e.to_string())?;
  }
 }
 for table in ["invoice_fabric_usage","worker_cut_entries","worker_tailor_entries","financial_entries"]{
  transaction.execute(&format!("DELETE FROM {table} WHERE invoice_id=?1"),[invoice_id]).map_err(|e|e.to_string())?;
 }
 transaction.execute("DELETE FROM invoices WHERE id=?1",[invoice_id]).map_err(|e|e.to_string())?;
 transaction.execute("DELETE FROM orders WHERE id=?1",[order_id]).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn delete_invoice(app:AppHandle,invoice_id:i64,customer_id:i64)->Result<(),String>{
 let mut conn=db(&app)?;
 delete_invoice_from_connection(&mut conn,invoice_id,customer_id)
}

#[cfg(test)]
mod invoice_deletion_tests{
 use super::*;
 #[test]
 fn deleting_one_invoice_restores_its_fabric_and_removes_linked_money(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch(
   "CREATE TABLE invoices(id INTEGER PRIMARY KEY,order_id INTEGER,customer_id INTEGER,invoice_number TEXT);
    CREATE TABLE orders(id INTEGER PRIMARY KEY);
    CREATE TABLE fabrics(id INTEGER PRIMARY KEY,stock_meters REAL);
    CREATE TABLE invoice_fabric_usage(invoice_id INTEGER,fabric_id INTEGER,meters REAL);
    CREATE TABLE fabric_movements(fabric_id INTEGER,movement_type TEXT,meters REAL,balance_after REAL,reference_type TEXT,reference_id INTEGER,notes TEXT,created_at TEXT);
    CREATE TABLE worker_cut_entries(invoice_id INTEGER);
    CREATE TABLE worker_tailor_entries(invoice_id INTEGER);
    CREATE TABLE financial_entries(invoice_id INTEGER,amount REAL);
    INSERT INTO invoices VALUES(1,1,1,'1'),(2,2,1,'2');
    INSERT INTO orders VALUES(1),(2);
    INSERT INTO fabrics VALUES(7,4);
    INSERT INTO invoice_fabric_usage VALUES(1,7,2),(2,7,3);
    INSERT INTO financial_entries VALUES(1,100),(2,60);"
  ).unwrap();
  assert!(delete_invoice_from_connection(&mut conn,1,2).is_err());
  delete_invoice_from_connection(&mut conn,1,1).unwrap();
  assert_eq!(conn.query_row("SELECT stock_meters FROM fabrics WHERE id=7",[],|row|row.get::<_,f64>(0)).unwrap(),6.0);
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM invoices WHERE id=2",[],|row|row.get::<_,i64>(0)).unwrap(),1);
  assert_eq!(conn.query_row("SELECT SUM(amount) FROM financial_entries",[],|row|row.get::<_,f64>(0)).unwrap(),60.0);
 }
 #[test]
 fn deleting_customer_removes_all_linked_invoices_and_restores_fabric(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY);
   CREATE TABLE invoices(id INTEGER PRIMARY KEY,order_id INTEGER,customer_id INTEGER,invoice_number TEXT);
   CREATE TABLE orders(id INTEGER PRIMARY KEY);
   CREATE TABLE fabrics(id INTEGER PRIMARY KEY,stock_meters REAL);
   CREATE TABLE invoice_fabric_usage(invoice_id INTEGER,fabric_id INTEGER,meters REAL);
   CREATE TABLE fabric_movements(fabric_id INTEGER,movement_type TEXT,meters REAL,balance_after REAL,reference_type TEXT,reference_id INTEGER,notes TEXT,created_at TEXT);
   CREATE TABLE worker_cut_entries(invoice_id INTEGER);
   CREATE TABLE worker_tailor_entries(invoice_id INTEGER);
   CREATE TABLE financial_entries(invoice_id INTEGER,amount REAL);
   INSERT INTO customers VALUES(1),(2);INSERT INTO orders VALUES(1),(2),(3);
   INSERT INTO invoices VALUES(1,1,1,'1'),(2,2,1,'2'),(3,3,2,'3');
   INSERT INTO fabrics VALUES(7,4);INSERT INTO invoice_fabric_usage VALUES(1,7,2),(2,7,3);
   INSERT INTO financial_entries VALUES(1,100),(2,60),(3,90);").unwrap();
  delete_customer_from_connection(&mut conn,1).unwrap();
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM customers",[],|r|r.get::<_,i64>(0)).unwrap(),1);
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM invoices",[],|r|r.get::<_,i64>(0)).unwrap(),1);
  assert_eq!(conn.query_row("SELECT stock_meters FROM fabrics WHERE id=7",[],|r|r.get::<_,f64>(0)).unwrap(),9.0);
  assert_eq!(conn.query_row("SELECT SUM(amount) FROM financial_entries",[],|r|r.get::<_,f64>(0)).unwrap(),90.0);
 }
}

#[tauri::command]
fn search_customers(app:AppHandle,query:String)->Result<Vec<CustomerSearchItem>,String>{
 let conn=db(&app)?;
 let query=query.trim().to_string();
 let escaped=query.replace('\\',"\\\\").replace('%',"\\%").replace('_',"\\_");
 let contains_pattern=format!("%{}%",escaped);
 let prefix_pattern=format!("{}%",escaped);
 let word_prefix_pattern=format!("% {}%",escaped);
 let mut statement=conn.prepare(&format!(
  "SELECT c.id,c.customer_code,c.name,c.phone,COUNT(i.id),COALESCE(MAX(i.created_at),''),
          COALESCE(SUM({INVOICE_REMAINING_EXPRESSION}),0)
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
 )).map_err(|e|e.to_string())?;
 let rows=statement.query_map(params![&query,&contains_pattern,&prefix_pattern,&word_prefix_pattern],|row|Ok(CustomerSearchItem{
  id:row.get(0)?,code:row.get(1)?,name:row.get(2)?,phone:row.get(3)?,
  invoice_count:row.get(4)?,last_invoice_at:row.get(5)?,outstanding:row.get(6)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn find_customer_by_phone(app:AppHandle,phone:String)->Result<Option<CustomerSearchItem>,String>{
 let conn=db(&app)?;
 find_customer_by_phone_from_connection(&conn,&phone)
}

fn find_customer_by_phone_from_connection(conn:&Connection,phone:&str)->Result<Option<CustomerSearchItem>,String>{
 let normalized=normalized_customer_phone(phone);
 if normalized.is_empty(){return Ok(None)}
 let mut statement=conn.prepare("SELECT id,phone FROM customers").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).map_err(|e|e.to_string())?;
 let mut customer_id=None;
 for row in rows{
  let (id,stored_phone)=row.map_err(|e|e.to_string())?;
  if normalized_customer_phone(&stored_phone)==normalized{customer_id=Some(id);break}
 }
 drop(statement);
 let Some(customer_id)=customer_id else{return Ok(None)};
 conn.query_row(&format!(
  "SELECT c.id,c.customer_code,c.name,c.phone,COUNT(i.id),COALESCE(MAX(i.created_at),''),
          COALESCE(SUM({INVOICE_REMAINING_EXPRESSION}),0)
   FROM customers c LEFT JOIN invoices i ON i.customer_id=c.id WHERE c.id=?1
   GROUP BY c.id,c.customer_code,c.name,c.phone"
 ),params![customer_id],|row|Ok(CustomerSearchItem{
  id:row.get(0)?,code:row.get(1)?,name:row.get(2)?,phone:row.get(3)?,
  invoice_count:row.get(4)?,last_invoice_at:row.get(5)?,outstanding:row.get(6)?,
 })).map(Some).map_err(|e|e.to_string())
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
          COALESCE((SELECT SUM(m.total_cost) FROM fabric_movements m JOIN fabrics sf ON sf.id=m.fabric_id WHERE sf.supplier_id=s.id AND m.meters>0),0),
          COALESCE((SELECT SUM(p.amount) FROM supplier_payments p WHERE p.supplier_id=s.id),0),s.created_at
   FROM suppliers s LEFT JOIN fabrics f ON f.supplier_id=s.id
   GROUP BY s.id,s.name,s.phone,s.notes,s.created_at ORDER BY s.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|{let total_purchases:f64=row.get(5)?;let total_paid:f64=row.get(6)?;Ok(Supplier{
  id:row.get(0)?,name:row.get(1)?,phone:row.get(2)?,notes:row.get(3)?,
  fabric_count:row.get(4)?,total_purchases,total_paid,balance:(total_purchases-total_paid).max(0.0),created_at:row.get(7)?,
 })}).map_err(|e|e.to_string())?;
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
  "INSERT INTO financial_entries(entry_type,description,amount,payment_method,created_at,reference_type,reference_id)
   VALUES('دفعة مورد',?1,?2,?3,datetime('now','localtime'),'دفعة مورد',?4)",
  params![format!("دفعة للمورد {}",supplier_name),amount,payment_method.trim(),id]
 ).map_err(|e|e.to_string())?;
 let created_at:String=transaction.query_row("SELECT created_at FROM supplier_payments WHERE id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(SupplierPayment{id,supplier_id,supplier_name,amount,payment_method,notes,created_at})
}

#[tauri::command]
fn list_supplier_ledger(app:AppHandle,supplier_id:i64)->Result<Vec<SupplierLedgerEntry>,String>{
 let conn=db(&app)?;
 let exists:i64=conn.query_row("SELECT COUNT(*) FROM suppliers WHERE id=?1",[supplier_id],|row|row.get(0)).map_err(|e|e.to_string())?;
 if exists==0{return Err("المورد غير موجود".into())}
 let mut statement=conn.prepare(
  "SELECT source_type,source_id,supplier_id,entry_type,title,details,amount,created_at
   FROM (
    SELECT 'supply' AS source_type,m.id AS source_id,f.supplier_id AS supplier_id,
           'توريد قماش' AS entry_type,
           CASE WHEN f.kind='ملون' THEN 'كتالوج ' || f.catalog_number || ' — لون ' || f.color_number
             ELSE f.name || CASE WHEN trim(f.color)<>'' THEN ' — ' || f.color ELSE '' END END AS title,
           m.movement_type || ' · ' || CASE m.entry_unit
             WHEN 'ياردة' THEN printf('%.2f ياردة',m.meters/0.9144)
             WHEN 'كرتون' THEN printf('%.2f كرتون × %.2f متر',m.carton_count,m.meters_per_carton)
             ELSE printf('%.2f متر',m.meters) END ||
             CASE WHEN trim(m.notes)<>'' THEN ' · ' || m.notes ELSE '' END AS details,
           m.total_cost AS amount,m.created_at AS created_at
    FROM fabric_movements m
    JOIN fabrics f ON f.id=m.fabric_id
    WHERE f.supplier_id=?1 AND m.meters>0 AND m.total_cost>0.0001
    UNION ALL
    SELECT 'payment',p.id,p.supplier_id,'تسديد للمورد','سند تسديد',
           p.payment_method || CASE WHEN trim(p.notes)<>'' THEN ' · ' || p.notes ELSE '' END,
           p.amount,p.created_at
    FROM supplier_payments p
    WHERE p.supplier_id=?1
   )
   ORDER BY datetime(created_at) DESC,source_id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([supplier_id],|row|Ok(SupplierLedgerEntry{
  source_type:row.get(0)?,source_id:row.get(1)?,supplier_id:row.get(2)?,entry_type:row.get(3)?,
  title:row.get(4)?,details:row.get(5)?,amount:row.get(6)?,created_at:row.get(7)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn update_supplier_ledger_date(app:AppHandle,source_type:String,source_id:i64,entry_date:String)->Result<(),String>{
 let date_text=entry_date.trim();
 let conn=db(&app)?;
 let valid:Option<String>=conn.query_row("SELECT date(?1)",[date_text],|row|row.get(0)).map_err(|e|e.to_string())?;
 let valid_date=valid.ok_or_else(||"التاريخ غير صحيح".to_string())?;
 match source_type.trim(){
  "supply"=>{
   let changed=conn.execute(
    "UPDATE fabric_movements
     SET created_at=?1 || ' ' || COALESCE(NULLIF(time(created_at),''),'12:00:00')
     WHERE id=?2",
    params![valid_date,source_id]
   ).map_err(|e|e.to_string())?;
   if changed==0{return Err("حركة التوريد غير موجودة".into())}
  },
  "payment"=>{
   let changed=conn.execute(
    "UPDATE supplier_payments
     SET created_at=?1 || ' ' || COALESCE(NULLIF(time(created_at),''),'12:00:00')
     WHERE id=?2",
    params![valid_date,source_id]
   ).map_err(|e|e.to_string())?;
   if changed==0{return Err("دفعة المورد غير موجودة".into())}
   conn.execute(
    "UPDATE financial_entries
     SET created_at=(SELECT created_at FROM supplier_payments WHERE id=?1)
     WHERE reference_type='دفعة مورد' AND reference_id=?1",
    [source_id]
   ).map_err(|e|e.to_string())?;
  },
  _=>return Err("نوع حركة المورد غير صحيح".into()),
 }
 Ok(())
}

#[tauri::command]
fn list_fabrics(app:AppHandle)->Result<Vec<FabricItem>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT f.id,f.supplier_id,COALESCE(s.name,''),f.name,f.color,f.stock_meters,
          f.purchase_price,f.sale_price,f.created_at,f.kind,f.catalog_number,f.color_number
   FROM fabrics f LEFT JOIN suppliers s ON s.id=f.supplier_id
   ORDER BY f.name,f.color,f.id"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(FabricItem{
  id:row.get(0)?,supplier_id:row.get(1)?,supplier_name:row.get(2)?,name:row.get(3)?,color:row.get(4)?,
  stock_meters:row.get(5)?,purchase_price:row.get(6)?,sale_price:row.get(7)?,created_at:row.get(8)?,
  kind:row.get(9)?,catalog_number:row.get(10)?,color_number:row.get(11)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn add_fabric(app:AppHandle,supplier_id:Option<i64>,name:String,color:String,kind:String,catalog_number:String,color_number:String,entry_unit:String,meter_quantity:f64,carton_count:f64,meters_per_carton:f64,purchase_amount:f64,sale_price:f64)->Result<i64,String>{
 let kind=kind.trim();
 if kind!="أبيض"&&kind!="ملون"{return Err("اختر القماش الأبيض أو الملون".into())}
 let catalog_number=catalog_number.trim();let color_number=color_number.trim();
 let name=if kind=="ملون"{catalog_number}else{name.trim()};
 let color=if kind=="ملون"{color_number}else{color.trim()};
 if name.is_empty()||color.is_empty(){return Err(if kind=="ملون"{"رقم الكتالوج ورقم اللون مطلوبان"}else{"اسم القماش واللون مطلوبان"}.into())}
 let (stock_meters,purchase_price,total_cost,normalized_unit)=supply_calculation(&entry_unit,meter_quantity,carton_count,meters_per_carton,purchase_amount)?;
 require_supplier_for_purchase(total_cost,supplier_id)?;
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let existing=transaction.query_row(
  "SELECT id,stock_meters,purchase_price FROM fabrics
   WHERE lower(trim(name))=lower(?1) AND lower(trim(color))=lower(?2) AND kind=?4
     AND ((supplier_id IS NULL AND ?3 IS NULL) OR supplier_id=?3)
   ORDER BY id LIMIT 1",
  params![name,color,supplier_id,kind],
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
  "INSERT INTO fabrics(supplier_id,name,color,stock_meters,purchase_price,sale_price,created_at,kind,catalog_number,color_number)
   VALUES(?1,?2,?3,?4,?5,?6,datetime('now','localtime'),?7,?8,?9)",
  params![supplier_id,name,color,stock_meters,purchase_price,sale_price.max(0.0),kind,if kind=="ملون"{catalog_number}else{""},if kind=="ملون"{color_number}else{""}]
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
 let normalized_unit=entry_unit.trim();
 if normalized_unit!="متر"&&normalized_unit!="ياردة"{return Err("اختر وحدة القماش: متر أو ياردة أو كرتون".into())}
 if !meter_quantity.is_finite()||meter_quantity<=0.0{return Err(format!("أدخل كمية القماش بوحدة {normalized_unit}"))}
 let meters=if normalized_unit=="ياردة"{meter_quantity*0.9144}else{meter_quantity};
 if !meters.is_finite()||!purchase_amount.is_finite()||(meter_quantity*purchase_amount).is_infinite(){return Err("كمية القماش أو سعره غير صحيح".into())}
 Ok((meters,(meter_quantity*purchase_amount)/meters,meter_quantity*purchase_amount,normalized_unit.into()))
}

fn require_supplier_for_purchase(total_cost:f64,supplier_id:Option<i64>)->Result<(),String>{
 if total_cost>0.0&&supplier_id.is_none(){Err("اختر المورد لتسجيل قيمة التوريد دينًا على المحل".into())}else{Ok(())}
}

#[cfg(test)]
mod fabric_unit_tests{
 use super::{supply_calculation,require_supplier_for_purchase,migrate_fabric_classification,consume_ready_fabric,reserve_ready_product};
 use rusqlite::Connection;
 #[test]
 fn older_fabrics_remain_white_and_migration_is_repeatable(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE fabrics(id INTEGER PRIMARY KEY,name TEXT,color TEXT);INSERT INTO fabrics(name,color) VALUES('الأجواد','سكري');").unwrap();
  migrate_fabric_classification(&conn).unwrap();migrate_fabric_classification(&conn).unwrap();
  let fields:(String,String,String)=conn.query_row("SELECT kind,catalog_number,color_number FROM fabrics",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
  assert_eq!(fields,("أبيض".into(),"".into(),"".into()));
 }
 #[test]
 fn ready_production_and_sale_update_both_inventories_without_overselling(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE fabrics(id INTEGER PRIMARY KEY,stock_meters REAL);CREATE TABLE ready_products(id INTEGER PRIMARY KEY,stock_quantity INTEGER);INSERT INTO fabrics VALUES(1,12);INSERT INTO ready_products VALUES(1,3);").unwrap();
  let tx=conn.transaction().unwrap();
  assert_eq!(consume_ready_fabric(&tx,1,7.0).unwrap(),5.0);
  assert!(consume_ready_fabric(&tx,1,6.0).is_err());
  reserve_ready_product(&tx,1,2).unwrap();
  assert!(reserve_ready_product(&tx,1,2).is_err());
  tx.commit().unwrap();
  let fabric:f64=conn.query_row("SELECT stock_meters FROM fabrics WHERE id=1",[],|r|r.get(0)).unwrap();
  let ready:i64=conn.query_row("SELECT stock_quantity FROM ready_products WHERE id=1",[],|r|r.get(0)).unwrap();
  assert_eq!((fabric,ready),(5.0,1));
 }
 #[test]
 fn paid_hundred_meters_create_thousand_riyals_supplier_debt(){
  let (meters,unit_cost,total_cost,unit)=supply_calculation("متر",100.0,0.0,0.0,10.0).unwrap();
  assert_eq!((meters,unit_cost,total_cost,unit),(100.0,10.0,1000.0,"متر".into()));
  assert!(require_supplier_for_purchase(total_cost,None).is_err());
  assert!(require_supplier_for_purchase(total_cost,Some(1)).is_ok());
 }
 #[test]
 fn yard_purchase_preserves_cost_and_converts_stock_to_meters(){
  let (meters,cost_per_meter,total,unit)=supply_calculation("ياردة",100.0,0.0,0.0,10.0).unwrap();
  assert!((meters-91.44).abs()<1e-9);
  assert!((total-1000.0).abs()<1e-9);
  assert!((cost_per_meter*meters-total).abs()<1e-9);
  assert_eq!(unit,"ياردة");
 }
 #[test]
 fn invalid_unit_does_not_enter_inventory(){
  assert!(supply_calculation("قدم",100.0,0.0,0.0,10.0).is_err());
 }
}

#[tauri::command]
fn restock_fabric(app:AppHandle,fabric_id:i64,entry_unit:String,meter_quantity:f64,carton_count:f64,meters_per_carton:f64,purchase_amount:f64,notes:String)->Result<(),String>{
 let (meters,purchase_price,total_cost,normalized_unit)=supply_calculation(&entry_unit,meter_quantity,carton_count,meters_per_carton,purchase_amount)?;
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let (current,supplier_id):(f64,Option<i64>)=transaction.query_row("SELECT stock_meters,supplier_id FROM fabrics WHERE id=?1",[fabric_id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(|_|"القماش غير موجود".to_string())?;
 require_supplier_for_purchase(total_cost,supplier_id)?;
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

fn money_cents(amount:f64)->Result<i64,String>{
 if !amount.is_finite()||amount<0.0||amount>10_000_000.0||(amount*100.0-(amount*100.0).round()).abs()>0.0001{
  return Err("اكتب مبلغًا صالحًا لا يتجاوز منزلتين عشريتين".into())
 }
 Ok((amount*100.0).round() as i64)
}

fn payment_parts(total:f64,method:&str,splits:Option<&[PaymentSplit]>)->Result<Vec<(String,f64)>,String>{
 let total_cents=money_cents(total)?;
 let Some(splits)=splits else{return Ok(if total_cents>0{vec![(method.to_string(),total)]}else{Vec::new()})};
 if splits.is_empty()||splits.len()>3{return Err("حدد مبالغ الدفع النقدي والشبكة والتحويل".into())}
 let mut seen=std::collections::HashSet::new();
 let mut parts=Vec::new();let mut sum=0i64;
 for part in splits{
  if !matches!(part.method.as_str(),"كاش"|"شبكة"|"تحويل")||!seen.insert(part.method.as_str()){
   return Err("طريقة الدفع المقسّم غير صحيحة".into())
  }
  let cents=money_cents(part.amount)?;
  if cents==0{return Err("يجب أن يكون مبلغ كل طريقة دفع أكبر من صفر".into())}
  sum+=cents;
  parts.push((part.method.clone(),cents as f64/100.0));
 }
 if sum!=total_cents{return Err("مجموع طرق الدفع لا يساوي المبلغ المدفوع".into())}
 Ok(parts)
}

#[cfg(test)]
mod payment_split_tests{
 use super::{invoice_payment_summary,payment_parts,PaymentSplit};
 use rusqlite::{params,Connection};

 #[test]
 fn split_payment_balances_and_rejects_mismatches(){
  let parts=[PaymentSplit{method:"كاش".into(),amount:50.0},PaymentSplit{method:"شبكة".into(),amount:50.0}];
  assert_eq!(payment_parts(100.0,"متعدد",Some(&parts)).unwrap(),vec![("كاش".to_string(),50.0),("شبكة".to_string(),50.0)]);
  assert!(payment_parts(99.0,"متعدد",Some(&parts)).is_err());
  assert!(payment_parts(100.0,"متعدد",Some(&[PaymentSplit{method:"كاش".into(),amount:50.0},PaymentSplit{method:"كاش".into(),amount:50.0}])).is_err());
 }

 #[test]
 fn financial_entries_keep_the_method_totals_separate(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE financial_entries(invoice_id INTEGER,entry_type TEXT,payment_method TEXT,amount REAL);").unwrap();
  let transaction=conn.transaction().unwrap();
  for (method,amount) in [("كاش",50.0),("شبكة",50.0)]{
   transaction.execute("INSERT INTO financial_entries(invoice_id,entry_type,payment_method,amount) VALUES(1,'دفعة فاتورة',?1,?2)",params![method,amount]).unwrap();
  }
  assert_eq!(invoice_payment_summary(&transaction,1).unwrap().unwrap(),"متعدد (كاش 50.00 + شبكة 50.00)");
 }
}

fn invoice_payment_summary(transaction:&rusqlite::Transaction<'_>,invoice_id:i64)->Result<Option<String>,String>{
 let mut statement=transaction.prepare("SELECT payment_method,SUM(amount) FROM financial_entries WHERE invoice_id=?1 AND entry_type='دفعة فاتورة' GROUP BY payment_method HAVING SUM(amount)>0.004 ORDER BY CASE payment_method WHEN 'كاش' THEN 0 WHEN 'شبكة' THEN 1 WHEN 'تحويل' THEN 2 ELSE 3 END,payment_method").map_err(|e|e.to_string())?;
 let rows=statement.query_map([invoice_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,f64>(1)?))).map_err(|e|e.to_string())?;
 let parts=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 Ok(match parts.len(){0=>None,1=>Some(parts[0].0.clone()),_=>Some(format!("متعدد ({})",parts.iter().map(|(method,amount)|format!("{method} {amount:.2}")).collect::<Vec<_>>().join(" + ")))})
}

const INVOICE_REMAINING_EXPRESSION:&str="MAX(0,COALESCE(CAST(NULLIF(i.total_price,'') AS REAL),0)-COALESCE(CAST(NULLIF(i.paid_amount,'') AS REAL),0)-COALESCE(CAST(NULLIF(i.discount,'') AS REAL),0))";

fn total_customer_debt(conn:&Connection)->Result<f64,String>{
 conn.query_row(&format!("SELECT COALESCE(SUM({INVOICE_REMAINING_EXPRESSION}),0) FROM invoices i"),[],|row|row.get(0)).map_err(|e|e.to_string())
}

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
 let total_outstanding=total_customer_debt(&conn)?;
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
fn add_financial_entry(app:AppHandle,entry_type:String,description:String,amount:f64,payment_method:String)->Result<FinancialEntry,String>{
 if entry_type!="مصروف"&&entry_type!="دخل يدوي"{return Err("نوع الحركة المالية غير صحيح".into())}
 if !amount.is_finite()||amount<=0.0{return Err("أدخل مبلغًا أكبر من صفر".into())}
 if description.trim().is_empty(){return Err("اكتب وصف الحركة المالية".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO financial_entries(entry_type,description,amount,payment_method,created_at) VALUES(?1,?2,?3,?4,datetime('now','localtime'))",params![entry_type,description.trim(),amount,payment_method]).map_err(|e|e.to_string())?;
 let id=conn.last_insert_rowid();
 conn.query_row("SELECT id,entry_type,description,amount,payment_method,created_at FROM financial_entries WHERE id=?1",[id],|row|Ok(FinancialEntry{id:row.get(0)?,entry_type:row.get(1)?,description:row.get(2)?,amount:row.get(3)?,payment_method:row.get(4)?,created_at:row.get(5)?})).map_err(|e|e.to_string())
}

#[tauri::command]
fn record_invoice_payment(app:AppHandle,invoice_id:i64,amount:f64,payment_method:String,payment_splits:Option<Vec<PaymentSplit>>)->Result<(),String>{
 if !amount.is_finite()||amount<=0.0{return Err("أدخل مبلغ الدفعة".into())}
 let parts=payment_parts(amount,&payment_method,payment_splits.as_deref())?;
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
 for (method,part_amount) in parts{
  transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![invoice_id,format!("فاتورة {} — {}",invoice_number,customer_name),part_amount,method]).map_err(|e|e.to_string())?;
 }
 if let Some(summary)=invoice_payment_summary(&transaction,invoice_id)?{
  transaction.execute("UPDATE invoices SET payment_method=?1 WHERE id=?2",params![summary,invoice_id]).map_err(|e|e.to_string())?;
 }
 transaction.commit().map_err(|e|e.to_string())
}

fn complete_delivery_from_connection(conn:&mut Connection,order_id:i64,allocations:Vec<DeliveryPaymentAllocation>)->Result<(),String>{
 let transaction=conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
 let context=delivery_context_from_connection(&transaction,order_id)?;
 let balances=std::iter::once(&context.current).chain(context.older.iter()).collect::<Vec<_>>();
 let mut seen=std::collections::HashSet::new();
 for allocation in allocations{
  if !seen.insert(allocation.invoice_id){return Err("لا تكرر الفاتورة في دفعة التسليم".into())}
  let balance=balances.iter().find(|item|item.invoice_id==allocation.invoice_id).ok_or("الفاتورة غير مرتبطة بهذا العميل أو ليست عليها ديون")?;
  let amount:f64=allocation.payment_splits.iter().map(|part|part.amount).sum();
  let cents=money_cents(amount)?;
  if cents==0{return Err("اكتب مبلغ الدفعة أو اترك الفاتورة دون دفع".into())}
  if cents>money_cents(balance.remaining)?{return Err(format!("المتبقي على الفاتورة {} {:.2} ريال فقط",balance.invoice_number,balance.remaining))}
  let parts=payment_parts(amount,"متعدد",Some(&allocation.payment_splits))?;
  let paid_text:String=transaction.query_row("SELECT paid_amount FROM invoices WHERE id=?1",[allocation.invoice_id],|row|row.get(0)).map_err(|e|e.to_string())?;
  transaction.execute("UPDATE invoices SET paid_amount=?1,updated_at=datetime('now','localtime') WHERE id=?2",params![format!("{:.2}",parse_money(&paid_text)+amount),allocation.invoice_id]).map_err(|e|e.to_string())?;
  for (method,part_amount) in parts{
   transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![allocation.invoice_id,format!("دفعة التسليم — فاتورة {} — {}",balance.invoice_number,context.customer_name),part_amount,method]).map_err(|e|e.to_string())?;
  }
  if let Some(summary)=invoice_payment_summary(&transaction,allocation.invoice_id)?{
   transaction.execute("UPDATE invoices SET payment_method=?1 WHERE id=?2",params![summary,allocation.invoice_id]).map_err(|e|e.to_string())?;
  }
 }
 update_order_stage(&transaction,order_id,"تم التسليم")?;
 transaction.commit().map_err(|e|e.to_string())
}

#[tauri::command]
fn complete_delivery(app:AppHandle,order_id:i64,allocations:Vec<DeliveryPaymentAllocation>)->Result<(),String>{
 complete_delivery_from_connection(&mut db(&app)?,order_id,allocations)
}

#[cfg(test)]
mod delivery_payment_tests{
 use super::{complete_delivery_from_connection,delivery_context_from_connection,DeliveryPaymentAllocation,PaymentSplit};
 use rusqlite::Connection;

 fn sample()->Connection{
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,name TEXT);CREATE TABLE orders(id INTEGER PRIMARY KEY,work_status TEXT,tailored_date TEXT,delivered_at TEXT,quantity INTEGER);CREATE TABLE invoices(id INTEGER PRIMARY KEY,order_id INTEGER,customer_id INTEGER,invoice_number TEXT,total_price TEXT,paid_amount TEXT,discount TEXT,updated_at TEXT,payment_method TEXT);CREATE TABLE financial_entries(entry_type TEXT,invoice_id INTEGER,description TEXT,amount REAL,payment_method TEXT,created_at TEXT);INSERT INTO customers VALUES(1,'عميل');INSERT INTO orders VALUES(1,'في المحل بانتظار التسليم',NULL,NULL,1),(2,'تم التسليم',NULL,'2026-09-01',1);INSERT INTO invoices VALUES(1,1,1,'101','140','40','0',NULL,'كاش'),(2,2,1,'90','300','0','0',NULL,'كاش');").unwrap();
  conn
 }

 #[test]
 fn split_current_and_old_debt_are_saved_with_delivery(){
  let mut conn=sample();
  let before=delivery_context_from_connection(&conn,1).unwrap();
  assert_eq!(before.current.remaining,100.0);
  assert_eq!(before.older[0].remaining,300.0);
  assert_eq!(before.total_debt,400.0);
  complete_delivery_from_connection(&mut conn,1,vec![
   DeliveryPaymentAllocation{invoice_id:1,payment_splits:vec![PaymentSplit{method:"كاش".into(),amount:50.0},PaymentSplit{method:"شبكة".into(),amount:50.0}]},
   DeliveryPaymentAllocation{invoice_id:2,payment_splits:vec![PaymentSplit{method:"شبكة".into(),amount:300.0}]},
  ]).unwrap();
  assert_eq!(conn.query_row("SELECT work_status FROM orders WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"تم التسليم");
  assert_eq!(conn.query_row("SELECT paid_amount FROM invoices WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"140.00");
  assert_eq!(conn.query_row("SELECT paid_amount FROM invoices WHERE id=2",[],|row|row.get::<_,String>(0)).unwrap(),"300.00");
  assert_eq!(conn.query_row("SELECT SUM(amount) FROM financial_entries",[],|row|row.get::<_,f64>(0)).unwrap(),400.0);
 }

 #[test]
 fn invalid_old_debt_payment_rolls_back_current_payment_and_delivery(){
  let mut conn=sample();
  let result=complete_delivery_from_connection(&mut conn,1,vec![
   DeliveryPaymentAllocation{invoice_id:1,payment_splits:vec![PaymentSplit{method:"كاش".into(),amount:100.0}]},
   DeliveryPaymentAllocation{invoice_id:2,payment_splits:vec![PaymentSplit{method:"شبكة".into(),amount:301.0}]},
  ]);
  assert!(result.is_err());
  assert_eq!(conn.query_row("SELECT work_status FROM orders WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"في المحل بانتظار التسليم");
  assert_eq!(conn.query_row("SELECT paid_amount FROM invoices WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"40");
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM financial_entries",[],|row|row.get::<_,i64>(0)).unwrap(),0);
 }

 #[test]
 fn delivery_without_payment_preserves_debt(){
  let mut conn=sample();
  complete_delivery_from_connection(&mut conn,1,vec![]).unwrap();
  assert_eq!(conn.query_row("SELECT work_status FROM orders WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"تم التسليم");
  assert_eq!(conn.query_row("SELECT paid_amount FROM invoices WHERE id=1",[],|row|row.get::<_,String>(0)).unwrap(),"40");
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM financial_entries",[],|row|row.get::<_,i64>(0)).unwrap(),0);
 }
}

#[tauri::command]
fn daily_report(app:AppHandle,report_date:String,end_date:String,period:String)->Result<DailyReport,String>{
 let conn=db(&app)?;
 report_from_connection(&conn,&report_date,&end_date,&period)
}

fn report_from_connection(conn:&Connection,report_date:&str,end_date:&str,period:&str)->Result<DailyReport,String>{
 let day=report_date.trim();let final_day=end_date.trim();
 if day.is_empty()||final_day.is_empty(){return Err("اختر تاريخ من وإلى".into())}
 for value in [day,final_day]{
  if value.len()!=10||value.as_bytes()[4]!=b'-'||value.as_bytes()[7]!=b'-'||!value.bytes().enumerate().all(|(index,byte)|index==4||index==7||byte.is_ascii_digit()){
   return Err("تاريخ التقرير غير صحيح".into())
  }
  let normalized:Option<String>=conn.query_row("SELECT date(?1,'+0 days')",[value],|row|row.get(0)).map_err(|e|e.to_string())?;
  if normalized.as_deref()!=Some(value){return Err("تاريخ التقرير غير صحيح".into())}
 }
 if final_day<day{return Err("تاريخ إلى يجب أن يكون بعد تاريخ من أو مساويًا له".into())}
 let period_label=match period{"يومي"=>"تقرير مالي يومي","شهري"=>"تقرير مالي شهري","سنوي"=>"تقرير مالي سنوي","مخصص"=>"تقرير مالي لفترة مخصصة",_=>return Err("فترة التقرير غير صحيحة".into())};
 let start=day.to_string();
 let upper_date=conn.query_row("SELECT date(?1,'+1 day')",[final_day],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?;
 let end_date=upper_date;
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
 Ok(DailyReport{report_date:day.into(),end_date:final_day.into(),period:period.into(),period_label:period_label.into(),new_customers,invoices,thobes,invoice_sales,invoice_received,extra_income,expenses,delivered,fabric_used,fabric_sold})
}

#[tauri::command]
fn get_app_settings(app:AppHandle)->Result<AppSettings,String>{
 let conn=db(&app)?;
 let value=|key:&str|->String{conn.query_row("SELECT setting_value FROM app_settings WHERE setting_key=?1",[key],|row|row.get(0)).unwrap_or_default()};
 let finance_pin=value("finance_pin");
 let app_pin=value("app_pin");
 let saved_theme=value("theme");
 let shop_name=value("shop_name");let owner_name=value("owner_name");let accountant_name=value("accountant_name");
 let large_cut_price=value("large_cut_price").parse::<f64>().ok().filter(|value|value.is_finite()&&*value>=0.0).unwrap_or(30.0);
 let small_cut_price=value("small_cut_price").parse::<f64>().ok().filter(|value|value.is_finite()&&*value>=0.0).unwrap_or(25.0);
 let auto_report_enabled=value("auto_report_enabled")=="true";
 let auto_report_time=match value("auto_report_time").as_str(){""=>"00:00".to_string(),time=>time.to_string()};
 let printer_check_minutes=value("printer_check_minutes").parse::<i64>().ok().filter(|minutes|*minutes>=1&&*minutes<=15).unwrap_or(3);
 let initialized=!shop_name.trim().is_empty()&&!owner_name.trim().is_empty()&&!finance_pin.is_empty()&&!app_pin.is_empty();
 Ok(AppSettings{shop_name,owner_name,accountant_name,finance_pin_set:!finance_pin.is_empty(),app_pin_set:!app_pin.is_empty(),theme:if saved_theme=="light"{"light".into()}else{"dark".into()},initialized,large_cut_price,small_cut_price,auto_report_enabled,auto_report_time,printer_check_minutes})
}

#[tauri::command]
fn save_auto_report_settings(app:AppHandle,enabled:bool,time:String,check_minutes:i64)->Result<AppSettings,String>{
 let valid_time=time.len()==5&&time.as_bytes()[2]==b':'&&time[..2].parse::<u8>().is_ok_and(|h|h<24)&&time[3..].parse::<u8>().is_ok_and(|m|m<60);
 if !valid_time||!(1..=15).contains(&check_minutes){return Err("حدد وقتًا صحيحًا وفترة فحص بين دقيقة و15 دقيقة".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 for (key,value) in [("auto_report_enabled",enabled.to_string()),("auto_report_time",time),("printer_check_minutes",check_minutes.to_string())]{
  transaction.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES(?1,?2) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",params![key,value]).map_err(|e|e.to_string())?;
 }
 transaction.commit().map_err(|e|e.to_string())?;
 get_app_settings(app)
}

#[tauri::command]
fn save_app_settings(app:AppHandle,shop_name:String,owner_name:String,accountant_name:String,finance_pin:String,app_pin:String)->Result<AppSettings,String>{
 let pin=finance_pin.trim();let entry_pin=app_pin.trim();
 if !pin.is_empty()&&(pin.len()<4||pin.len()>8||!pin.chars().all(|character|character.is_ascii_digit())){return Err("رمز المالية يجب أن يكون من 4 إلى 8 أرقام إنجليزية".into())}
 if !entry_pin.is_empty()&&(entry_pin.len()<4||entry_pin.len()>8||!entry_pin.chars().all(|character|character.is_ascii_digit())){return Err("رمز دخول التطبيق يجب أن يكون من 4 إلى 8 أرقام إنجليزية".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('shop_name',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[shop_name.trim()]).map_err(|e|e.to_string())?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('owner_name',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[owner_name.trim()]).map_err(|e|e.to_string())?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('accountant_name',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[accountant_name.trim()]).map_err(|e|e.to_string())?;
 if !pin.is_empty(){conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('finance_pin',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[pin]).map_err(|e|e.to_string())?;}
 if !entry_pin.is_empty(){conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('app_pin',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[entry_pin]).map_err(|e|e.to_string())?;}
 get_app_settings(app)
}

#[tauri::command]
fn save_cut_prices(app:AppHandle,large_cut_price:f64,small_cut_price:f64)->Result<AppSettings,String>{
 if !large_cut_price.is_finite()||large_cut_price<0.0||!small_cut_price.is_finite()||small_cut_price<0.0{return Err("أجور الخياط يجب أن تكون أرقامًا صحيحة وغير سالبة".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('large_cut_price',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[large_cut_price.to_string()]).map_err(|e|e.to_string())?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('small_cut_price',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[small_cut_price.to_string()]).map_err(|e|e.to_string())?;
 get_app_settings(app)
}

fn read_worker(row:&rusqlite::Row<'_>)->rusqlite::Result<WorkerProfile>{
 Ok(WorkerProfile{id:row.get(0)?,name:row.get(1)?,role:row.get(2)?,pay_mode:row.get(3)?,
  monthly_salary:row.get(4)?,large_rate:row.get(5)?,small_rate:row.get(6)?,
  salary_start:row.get(7)?,active:row.get::<_,i64>(8)?!=0,archived_at:row.get(9)?,created_at:row.get(10)?})
}
fn worker_from_db(conn:&Connection,id:i64)->Result<WorkerProfile,String>{
 conn.query_row("SELECT id,name,role,pay_mode,monthly_salary,large_rate,small_rate,salary_start,active,archived_at,created_at FROM workers WHERE id=?1",[id],read_worker).map_err(|_|"العامل غير موجود".into())
}
#[tauri::command]
fn list_workers(app:AppHandle)->Result<Vec<WorkerProfile>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT id,name,role,pay_mode,monthly_salary,large_rate,small_rate,salary_start,active,archived_at,created_at FROM workers ORDER BY active DESC,id DESC").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],read_worker).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
#[tauri::command]
fn list_worker_names(app:AppHandle)->Result<Vec<String>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT name FROM workers WHERE active=1 AND role='قصاص' ORDER BY id ASC").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}
#[tauri::command]
fn save_worker(app:AppHandle,id:Option<i64>,name:String,role:String,pay_mode:String,monthly_salary:f64,large_rate:f64,small_rate:f64,salary_start:String)->Result<WorkerProfile,String>{
 let name=name.trim();
 if name.is_empty(){return Err("اكتب اسم العامل".into())}
 if role!="قصاص"&&role!="خياط"{return Err("اختر وظيفة العامل".into())}
 if pay_mode!="قطعة"&&pay_mode!="راتب"{return Err("اختر طريقة الأجر".into())}
 if !monthly_salary.is_finite()||monthly_salary<0.0||!large_rate.is_finite()||large_rate<0.0||!small_rate.is_finite()||small_rate<0.0{return Err("قيمة الأجر غير صحيحة".into())}
 if pay_mode=="راتب"&&monthly_salary<=0.0{return Err("حدد راتبًا شهريًا أكبر من صفر".into())}
 let month=salary_start.trim();
 let digits=month.as_bytes();
 let valid_month=digits.len()==7&&digits[4]==b'-'&&digits[..4].iter().all(u8::is_ascii_digit)&&digits[5..].iter().all(u8::is_ascii_digit)&&month[5..].parse::<u8>().is_ok_and(|m|m>=1&&m<=12);
 if !valid_month{return Err("شهر بداية الراتب غير صحيح".into())}
 let mut conn=db(&app)?;
 let current_month:String=conn.query_row("SELECT strftime('%Y-%m','now','localtime')",[],|row|row.get(0)).map_err(|e|e.to_string())?;
 if month>current_month.as_str(){return Err("لا يمكن بدء الراتب في شهر مستقبلي".into())}
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let (worker_id,effective_month)=if let Some(worker_id)=id{
  let (saved_name,saved_role):(String,String)=transaction.query_row("SELECT name,role FROM workers WHERE id=?1",[worker_id],|row|Ok((row.get(0)?,row.get(1)?))).map_err(|_|"العامل غير موجود".to_string())?;
  if saved_name!=name{return Err("لا يمكن تغيير اسم العامل بعد حفظ الفواتير. أضف عاملًا جديدًا بالاسم الجديد.".into())}
  if saved_role!=role{return Err("وظيفة العامل محفوظة في سجلات الفواتير. أضف عاملًا جديدًا لوظيفة مختلفة.".into())}
  transaction.execute("DELETE FROM worker_salary_changes WHERE worker_id=?1 AND effective_month>?2",params![worker_id,current_month]).map_err(|e|e.to_string())?;
  transaction.execute("UPDATE workers SET role=?1,pay_mode=?2,monthly_salary=?3,large_rate=?4,small_rate=?5,active=1,archived_at='' WHERE id=?6",params![role,pay_mode,monthly_salary,large_rate,small_rate,worker_id]).map_err(|e|e.to_string())?;
  (worker_id,current_month)
 }else{
  transaction.execute("INSERT INTO workers(name,role,pay_mode,monthly_salary,large_rate,small_rate,salary_start,active,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,1,datetime('now','localtime'))",
   params![name,role,pay_mode,monthly_salary,large_rate,small_rate,format!("{month}-01")]).map_err(|e|if e.to_string().contains("UNIQUE"){ "الاسم موجود بالفعل. افتح ملف العامل لتعديله أو تفعيله.".into() }else{ e.to_string() })?;
  (transaction.last_insert_rowid(),month.to_string())
 };
 transaction.execute("INSERT INTO worker_salary_changes(worker_id,effective_month,monthly_salary) VALUES(?1,?2,?3) ON CONFLICT(worker_id,effective_month) DO UPDATE SET monthly_salary=excluded.monthly_salary",
  params![worker_id,effective_month,if pay_mode=="راتب"{monthly_salary}else{0.0}]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 worker_from_db(&conn,worker_id)
}
#[tauri::command]
fn add_worker(app:AppHandle,name:String)->Result<Vec<String>,String>{
 let normalized=name.trim();
 if normalized.is_empty(){return Err("اكتب اسم العامل".into())}
 let conn=db(&app)?;
 conn.execute("INSERT INTO workers(name,created_at,salary_start,large_rate,small_rate) VALUES(?1,datetime('now','localtime'),date('now','localtime','start of month'),COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='large_cut_price'),30),COALESCE((SELECT CAST(setting_value AS REAL) FROM app_settings WHERE setting_key='small_cut_price'),25)) ON CONFLICT(name) DO UPDATE SET active=1,archived_at=''",[normalized]).map_err(|e|e.to_string())?;
 list_worker_names(app)
}
#[tauri::command]
fn delete_worker(app:AppHandle,name:String)->Result<Vec<String>,String>{
 let normalized=name.trim();
 if normalized.is_empty(){return Err("اسم العامل غير صحيح".into())}
 let conn=db(&app)?;
 conn.execute("UPDATE workers SET active=0,archived_at=datetime('now','localtime') WHERE name=?1",[normalized]).map_err(|e|e.to_string())?;
 conn.execute("INSERT OR REPLACE INTO worker_salary_changes(worker_id,effective_month,monthly_salary) SELECT id,strftime('%Y-%m',date('now','localtime','start of month','+1 month')),0 FROM workers WHERE name=?1",[normalized]).map_err(|e|e.to_string())?;
 list_worker_names(app)
}
#[tauri::command]
fn archive_worker(app:AppHandle,worker_id:i64)->Result<(),String>{
 let conn=db(&app)?;
 let updated=conn.execute("UPDATE workers SET active=0,archived_at=datetime('now','localtime') WHERE id=?1",[worker_id]).map_err(|e|e.to_string())?;
 if updated==0{return Err("العامل غير موجود".into())}
 conn.execute("INSERT OR REPLACE INTO worker_salary_changes(worker_id,effective_month,monthly_salary) VALUES(?1,strftime('%Y-%m',date('now','localtime','start of month','+1 month')),0)",[worker_id]).map_err(|e|e.to_string())?;
 Ok(())
}
#[tauri::command]
fn worker_account(app:AppHandle,worker_id:i64)->Result<WorkerAccount,String>{
 let conn=db(&app)?;
 let worker=worker_from_db(&conn,worker_id)?;
 let mut statement=conn.prepare(
  "SELECT w.id,'قص',printf('فاتورة %s · الثوب %d',i.invoice_number,w.thobe_index+1),w.amount,w.created_at,i.invoice_number,COALESCE(c.name,''),w.thobe_index,w.thobe_size,''
   FROM worker_cut_entries w JOIN invoices i ON i.id=w.invoice_id LEFT JOIN customers c ON c.id=i.customer_id WHERE w.worker_name=?1 AND w.amount>0
   UNION ALL
   SELECT t.id,'خياطة',printf('فاتورة %s · الثوب %d',i.invoice_number,t.thobe_index+1),t.amount,t.created_at,i.invoice_number,COALESCE(c.name,''),t.thobe_index,t.thobe_size,''
   FROM worker_tailor_entries t JOIN invoices i ON i.id=t.invoice_id LEFT JOIN customers c ON c.id=i.customer_id WHERE t.worker_id=?2 AND t.amount>0
   UNION ALL
   SELECT d.id,CASE d.kind WHEN 'نقدي' THEN 'سحب نقدي' ELSE 'سحب عيني' END,d.details,-d.amount,d.created_at,'','',-1,'',d.payment_method
   FROM worker_withdrawals d WHERE d.worker_id=?2
   ORDER BY created_at ASC,id ASC"
 ).map_err(|e|e.to_string())?;
 let movements=statement.query_map(params![&worker.name,worker_id],|row|Ok(WorkerMovement{
  id:row.get(0)?,entry_type:row.get(1)?,description:row.get(2)?,amount:row.get(3)?,created_at:row.get(4)?,
  invoice_number:row.get(5)?,customer_name:row.get(6)?,thobe_index:row.get(7)?,thobe_size:row.get(8)?,payment_method:row.get(9)?
 })).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 let mut salary_query=conn.prepare("SELECT effective_month,monthly_salary FROM worker_salary_changes WHERE worker_id=?1 ORDER BY effective_month").map_err(|e|e.to_string())?;
 let salary_changes=salary_query.query_map([worker_id],|row|Ok(WorkerSalaryChange{effective_month:row.get(0)?,monthly_salary:row.get(1)?})).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
 Ok(WorkerAccount{worker,movements,salary_changes})
}
#[tauri::command]
fn record_worker_withdrawal(app:AppHandle,worker_id:i64,kind:String,amount:f64,details:String,payment_method:String)->Result<(),String>{
 if kind!="نقدي"&&kind!="عيني"{return Err("حدد نوع السحب".into())}
 if !amount.is_finite()||amount<=0.0{return Err("قيمة السحب يجب أن تكون أكبر من صفر".into())}
 if kind=="عيني"&&details.trim().is_empty(){return Err("اكتب وصف الشيء الذي سحبه العامل".into())}
 if kind=="نقدي"&&payment_method!="كاش"&&payment_method!="شبكة"&&payment_method!="تحويل"{return Err("طريقة الدفع غير صحيحة".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let name:String=transaction.query_row("SELECT name FROM workers WHERE id=?1 AND active=1",[worker_id],|row|row.get(0)).map_err(|_|"العامل غير موجود أو مؤرشف".to_string())?;
 transaction.execute("INSERT INTO worker_withdrawals(worker_id,kind,amount,details,payment_method,created_at) VALUES(?1,?2,?3,?4,?5,datetime('now','localtime'))",
  params![worker_id,kind,amount,details.trim(),if kind=="نقدي"{payment_method.as_str()}else{""}]).map_err(|e|e.to_string())?;
 let draw_id=transaction.last_insert_rowid();
 if kind=="نقدي"{
  transaction.execute("INSERT INTO financial_entries(entry_type,description,amount,payment_method,created_at,reference_type,reference_id) VALUES('مصروف',?1,?2,?3,datetime('now','localtime'),'سحب عامل',?4)",
   params![format!("سحب العامل {name} — {}",details.trim()),amount,payment_method,draw_id]).map_err(|e|e.to_string())?;
 }
 transaction.commit().map_err(|e|e.to_string())
}

#[tauri::command]
fn worker_ledger(app:AppHandle,name:Option<String>)->Result<Vec<WorkerLedgerEntry>,String>{
 let filter=name.unwrap_or_default();
 let worker=filter.trim();
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT w.id,w.invoice_id,i.invoice_number,COALESCE(c.name,''),w.thobe_index,w.thobe_size,w.amount,w.created_at,w.worker_name
   FROM worker_cut_entries w
   JOIN invoices i ON i.id=w.invoice_id
   LEFT JOIN customers c ON c.id=i.customer_id
   WHERE (?1='' OR w.worker_name=?1)
   ORDER BY datetime(w.created_at) DESC,w.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([worker],|row|Ok(WorkerLedgerEntry{
  id:row.get(0)?,invoice_id:row.get(1)?,invoice_number:row.get(2)?,customer_name:row.get(3)?,
  thobe_index:row.get(4)?,thobe_size:row.get(5)?,amount:row.get(6)?,created_at:row.get(7)?,
  worker_name:row.get(8)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
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
fn admin_reset_pin(app:AppHandle,target:String,new_pin:String,admin_code:String)->Result<AppSettings,String>{
 let key=admin_reset_key(&target,&admin_code)?;
 let pin=new_pin.trim();
 if pin.len()<4||pin.len()>8||!pin.chars().all(|character|character.is_ascii_digit()){return Err("الرمز الجديد يجب أن يكون من 4 إلى 8 أرقام إنجليزية".into())}
 let conn=db(&app)?;conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES(?1,?2) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",params![key,pin]).map_err(|e|e.to_string())?;
 get_app_settings(app)
}

fn admin_reset_key(target:&str,admin_code:&str)->Result<&'static str,String>{
 if admin_code!="ADMIN"{return Err("رمز الإدارة غير صحيح. اكتب ADMIN بالأحرف الكبيرة.".into())}
 match target{"app"=>Ok("app_pin"),"finance"=>Ok("finance_pin"),_=>Err("نوع الرمز غير صحيح".into())}
}

#[cfg(test)]
mod admin_pin_tests{
 use super::admin_reset_key;
 #[test]
 fn reset_requires_exact_admin_code_and_targets_one_pin(){
  assert_eq!(admin_reset_key("app","ADMIN").unwrap(),"app_pin");
  assert_eq!(admin_reset_key("finance","ADMIN").unwrap(),"finance_pin");
  assert!(admin_reset_key("app","admin").is_err());
  assert!(admin_reset_key("finance","Admin").is_err());
  assert!(admin_reset_key("both","ADMIN").is_err());
 }
}

#[tauri::command]
fn save_theme(app:AppHandle,theme:String)->Result<AppSettings,String>{
 let value=if theme=="light"{"light"}else{"dark"};
 let conn=db(&app)?;
 conn.execute("INSERT INTO app_settings(setting_key,setting_value) VALUES('theme',?1) ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",[value]).map_err(|e|e.to_string())?;
 get_app_settings(app)
}

#[tauri::command]
fn list_ready_products(app:AppHandle)->Result<Vec<ReadyProduct>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT p.id,p.name,p.size,p.fabric_id,CASE WHEN f.kind='ملون' THEN 'كتالوج '||f.catalog_number||' — لون '||f.color_number ELSE f.name||' — '||f.color END,p.stock_quantity,p.sale_price,p.created_at FROM ready_products p JOIN fabrics f ON f.id=p.fabric_id ORDER BY p.id DESC").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ReadyProduct{id:row.get(0)?,name:row.get(1)?,size:row.get(2)?,fabric_id:row.get(3)?,fabric_label:row.get(4)?,stock_quantity:row.get(5)?,sale_price:row.get(6)?,created_at:row.get(7)?})).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

fn ready_length(measurements:&serde_json::Value,key:&str,unit:&str)->Result<f64,String>{
 let value=measurements.get(key).and_then(serde_json::Value::as_str).unwrap_or("").trim();
 let number=value.replace(',',".").parse::<f64>().map_err(|_|format!("أدخل {key} بالأرقام"))?;
 if !number.is_finite()||number<=0.0{return Err(format!("{key} غير صحيح"))}
 Ok(if unit=="سم"{number/2.54}else{number})
}

#[tauri::command]
fn list_ready_items(app:AppHandle)->Result<Vec<ReadyItem>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT p.id,p.stock_quantity,p.initial_quantity,p.length_inches,p.width_inches,p.unit,
          p.measurements_json,p.designs_json,p.details_json,p.thobe_type,p.fabric_id,
          CASE WHEN f.kind='ملون' THEN 'كتالوج '||f.catalog_number||' — لون '||f.color_number ELSE f.name||' — '||f.color END,
          p.sale_price,p.created_at
   FROM ready_products p JOIN fabrics f ON f.id=p.fabric_id WHERE p.stock_quantity>0
   ORDER BY p.length_inches,p.width_inches,p.id DESC"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ReadyItem{
  id:row.get(0)?,quantity:row.get(1)?,initial_quantity:row.get(2)?,
  length_inches:row.get(3)?,width_inches:row.get(4)?,unit:row.get(5)?,
  measurements_json:row.get(6)?,designs_json:row.get(7)?,details_json:row.get(8)?,
  thobe_type:row.get(9)?,fabric_id:row.get(10)?,fabric_label:row.get(11)?,
  sale_price:row.get(12)?,created_at:row.get(13)?
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

fn insert_ready_item(conn:&mut Connection,payload:ReadyItemPayload)->Result<i64,String>{
 if payload.quantity<1||payload.quantity>1000{return Err("عدد الثياب الجاهزة غير صحيح".into())}
 if payload.unit!="إنش"&&payload.unit!="سم"{return Err("وحدة المقاسات غير صحيحة".into())}
 if !payload.sale_price.is_finite()||payload.sale_price<0.0{return Err("سعر البيع غير صحيح".into())}
 if !payload.fabric_meters.is_finite()||payload.fabric_meters<=0.0{return Err("أدخل استهلاك القماش بالمتر للثوب الواحد".into())}
 let fabric_id=payload.fabric_id.ok_or_else(||"اختر القماش للتفصيل الجاهز".to_string())?;
 let measurements:serde_json::Value=serde_json::from_str(&payload.measurements_json).map_err(|_|"المقاسات غير صالحة".to_string())?;
 let length=ready_length(&measurements,"طول أمام",&payload.unit)?;
 let width=ready_length(&measurements,"مقاس العرض",&payload.unit)?;
 let designs:serde_json::Value=serde_json::from_str(&payload.designs_json).map_err(|_|"الأشكال غير صالحة".to_string())?;
 let details:serde_json::Value=serde_json::from_str(&payload.details_json).map_err(|_|"تفاصيل الجاهز غير صالحة".to_string())?;
 if !measurements.is_object()||!designs.is_object()||!details.is_object(){return Err("بيانات الثوب غير مكتملة".into())}
 let used=payload.quantity as f64*payload.fabric_meters;
 if !used.is_finite(){return Err("استهلاك القماش غير صحيح".into())}
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let balance=consume_ready_fabric(&transaction,fabric_id,used)?;
 transaction.execute(
  "INSERT INTO ready_products(name,size,fabric_id,stock_quantity,initial_quantity,length_inches,width_inches,unit,measurements_json,designs_json,details_json,thobe_type,sale_price,created_at)
   VALUES('جاهز',?1,?2,?3,?3,?4,?5,?6,?7,?8,?9,?10,?11,datetime('now','localtime'))",
  params![format!("{length:.2}"),fabric_id,payload.quantity,length,width,payload.unit,payload.measurements_json,payload.designs_json,payload.details_json,payload.thobe_type.trim(),payload.sale_price]
 ).map_err(|e|e.to_string())?;
 let product_id=transaction.last_insert_rowid();
 transaction.execute(
  "INSERT INTO ready_productions(product_id,quantity,fabric_meters,tailor_name,created_at)
   VALUES(?1,?2,?3,'',datetime('now','localtime'))",
  params![product_id,payload.quantity,used]
 ).map_err(|e|e.to_string())?;
 let production_id=transaction.last_insert_rowid();
 transaction.execute(
  "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
   VALUES(?1,'تفصيل جاهز',?2,?3,'تفصيل جاهز',?4,?5,datetime('now','localtime'))",
  params![fabric_id,-used,balance,production_id,format!("{} ثوب جاهز",payload.quantity)]
 ).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(product_id)
}

#[tauri::command]
fn add_ready_item(app:AppHandle,payload:ReadyItemPayload)->Result<i64,String>{
 let mut conn=db(&app)?;
 insert_ready_item(&mut conn,payload)
}

#[tauri::command]
fn delete_ready_item(app:AppHandle,id:i64)->Result<(),String>{
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let sales:i64=transaction.query_row("SELECT COUNT(*) FROM extra_transactions WHERE ready_product_id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 if sales>0{return Err("لا يمكن حذف جاهز بيعت منه قطع؛ سجل البيع محفوظ".into())}
 let (fabric_id,quantity,initial,measurements):(i64,i64,i64,String)=transaction.query_row(
  "SELECT fabric_id,stock_quantity,initial_quantity,measurements_json FROM ready_products WHERE id=?1",
  [id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))
 ).map_err(|_|"الجاهز غير موجود".to_string())?;
 if measurements=="{}"||quantity!=initial{return Err("حذف هذا الجاهز القديم غير متاح؛ احتفظ بسجل التوريد والبيع".into())}
 let total:f64=transaction.query_row("SELECT SUM(fabric_meters) FROM ready_productions WHERE product_id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.execute("UPDATE fabrics SET stock_meters=stock_meters+?1 WHERE id=?2",params![total,fabric_id]).map_err(|e|e.to_string())?;
 let balance:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.execute(
  "INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at)
   VALUES(?1,'إلغاء جاهز',?2,?3,'إلغاء جاهز',?4,'استرجاع قماش الجاهز',datetime('now','localtime'))",
  params![fabric_id,total,balance,id]
 ).map_err(|e|e.to_string())?;
 transaction.execute("DELETE FROM ready_productions WHERE product_id=?1",[id]).map_err(|e|e.to_string())?;
 transaction.execute("DELETE FROM ready_products WHERE id=?1",[id]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())
}

#[cfg(test)]
mod ready_measurement_tests{
 use super::*;
 #[test]
 fn measured_ready_stock_deducts_fabric_and_can_be_sold_without_customer(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch(
   "CREATE TABLE fabrics(id INTEGER PRIMARY KEY,stock_meters REAL);
    CREATE TABLE ready_products(id INTEGER PRIMARY KEY,name TEXT,size TEXT,fabric_id INTEGER,stock_quantity INTEGER,initial_quantity INTEGER,length_inches REAL,width_inches REAL,unit TEXT,measurements_json TEXT,designs_json TEXT,details_json TEXT,thobe_type TEXT,sale_price REAL,created_at TEXT);
    CREATE TABLE ready_productions(id INTEGER PRIMARY KEY,product_id INTEGER,quantity INTEGER,fabric_meters REAL,tailor_name TEXT,created_at TEXT);
    CREATE TABLE fabric_movements(fabric_id INTEGER,movement_type TEXT,meters REAL,balance_after REAL,reference_type TEXT,reference_id INTEGER,notes TEXT,created_at TEXT);
    INSERT INTO fabrics VALUES(1,20);"
  ).unwrap();
  let id=insert_ready_item(&mut conn,ReadyItemPayload{
   quantity:3,unit:"سم".into(),measurements_json:r#"{"طول أمام":"147.32","مقاس العرض":"50.8"}"#.into(),
   designs_json:"{}".into(),details_json:"{}".into(),thobe_type:"سعودي".into(),
   fabric_id:Some(1),fabric_meters:2.0,sale_price:90.0
  }).unwrap();
  let (length,width):(f64,f64)=conn.query_row("SELECT length_inches,width_inches FROM ready_products WHERE id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
  assert!((length-58.0).abs()<1e-9&&width==20.0);
  assert!(length>=57.0&&length<=59.0);
  assert_eq!(conn.query_row("SELECT stock_meters FROM fabrics WHERE id=1",[],|row|row.get::<_,f64>(0)).unwrap(),14.0);
  let transaction=conn.transaction().unwrap();
  assert!(reserve_ready_product(&transaction,id,4).is_err());
  reserve_ready_product(&transaction,id,2).unwrap();
  transaction.commit().unwrap();
  assert_eq!(conn.query_row("SELECT stock_quantity FROM ready_products WHERE id=?1",[id],|row|row.get::<_,i64>(0)).unwrap(),1);
 }
 #[test]
 fn insufficient_fabric_rolls_back_new_ready_product(){
  let mut conn=Connection::open_in_memory().unwrap();
  conn.execute_batch(
   "CREATE TABLE fabrics(id INTEGER PRIMARY KEY,stock_meters REAL);
    CREATE TABLE ready_products(id INTEGER PRIMARY KEY,name TEXT,size TEXT,fabric_id INTEGER,stock_quantity INTEGER,initial_quantity INTEGER,length_inches REAL,width_inches REAL,unit TEXT,measurements_json TEXT,designs_json TEXT,details_json TEXT,thobe_type TEXT,sale_price REAL,created_at TEXT);
    CREATE TABLE ready_productions(id INTEGER PRIMARY KEY,product_id INTEGER,quantity INTEGER,fabric_meters REAL,tailor_name TEXT,created_at TEXT);
    CREATE TABLE fabric_movements(fabric_id INTEGER,movement_type TEXT,meters REAL,balance_after REAL,reference_type TEXT,reference_id INTEGER,notes TEXT,created_at TEXT);
    INSERT INTO fabrics VALUES(1,3);"
  ).unwrap();
  let result=insert_ready_item(&mut conn,ReadyItemPayload{
   quantity:2,unit:"إنش".into(),measurements_json:r#"{"طول أمام":"58","مقاس العرض":"20"}"#.into(),
   designs_json:"{}".into(),details_json:"{}".into(),thobe_type:"سعودي".into(),
   fabric_id:Some(1),fabric_meters:2.0,sale_price:90.0
  });
  assert!(result.is_err());
  assert_eq!(conn.query_row("SELECT COUNT(*) FROM ready_products",[],|row|row.get::<_,i64>(0)).unwrap(),0);
 }
}

#[tauri::command]
fn list_ready_productions(app:AppHandle)->Result<Vec<ReadyProduction>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare("SELECT b.id,b.product_id,p.name,p.size,b.quantity,b.fabric_meters,b.tailor_name,b.created_at FROM ready_productions b JOIN ready_products p ON p.id=b.product_id ORDER BY b.id DESC LIMIT 100").map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ReadyProduction{id:row.get(0)?,product_id:row.get(1)?,product_name:row.get(2)?,size:row.get(3)?,quantity:row.get(4)?,fabric_meters:row.get(5)?,tailor_name:row.get(6)?,created_at:row.get(7)?})).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

fn consume_ready_fabric(transaction:&rusqlite::Transaction<'_>,fabric_id:i64,meters:f64)->Result<f64,String>{
 let stock:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|_|"القماش غير موجود".to_string())?;
 if stock+0.0001<meters{return Err(format!("المتوفر {:.2} متر فقط؛ التفصيل يحتاج {:.2} متر",stock,meters))}
 let balance=stock-meters;
 transaction.execute("UPDATE fabrics SET stock_meters=?1 WHERE id=?2",params![balance,fabric_id]).map_err(|e|e.to_string())?;
 Ok(balance)
}

fn reserve_ready_product(transaction:&rusqlite::Transaction<'_>,product_id:i64,quantity:i64)->Result<(),String>{
 let available:i64=transaction.query_row("SELECT stock_quantity FROM ready_products WHERE id=?1",[product_id],|row|row.get(0)).map_err(|_|"صنف الجاهز غير موجود".to_string())?;
 if quantity<1||quantity>available{return Err(format!("المتوفر {} قطع فقط من هذا الجاهز",available))}
 transaction.execute("UPDATE ready_products SET stock_quantity=stock_quantity-?1 WHERE id=?2",params![quantity,product_id]).map_err(|e|e.to_string())?;
 Ok(())
}

#[tauri::command]
fn add_ready_production(app:AppHandle,name:String,size:String,fabric_id:i64,quantity:i64,meters_per_piece:f64,tailor_name:String,sale_price:f64)->Result<i64,String>{
 let name=name.trim();let size=size.trim();
 if name.is_empty()||size.is_empty(){return Err("اسم الجاهز ومقاسه مطلوبان".into())}
 if quantity<=0||quantity>1000||!meters_per_piece.is_finite()||meters_per_piece<=0.0||!sale_price.is_finite()||sale_price<0.0{return Err("الكمية أو الاستهلاك أو السعر غير صحيح".into())}
 let total_meters=quantity as f64*meters_per_piece;
 if !total_meters.is_finite(){return Err("استهلاك القماش غير صحيح".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let balance=consume_ready_fabric(&transaction,fabric_id,total_meters)?;
 let existing=transaction.query_row("SELECT id,stock_quantity FROM ready_products WHERE lower(trim(name))=lower(?1) AND lower(trim(size))=lower(?2) AND fabric_id=?3 ORDER BY id LIMIT 1",params![name,size,fabric_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).optional().map_err(|e|e.to_string())?;
 let product_id=if let Some((id,stock_quantity))=existing{
  transaction.execute("UPDATE ready_products SET stock_quantity=?1,sale_price=?2 WHERE id=?3",params![stock_quantity+quantity,sale_price,id]).map_err(|e|e.to_string())?;id
 }else{
  transaction.execute("INSERT INTO ready_products(name,size,fabric_id,stock_quantity,sale_price,created_at) VALUES(?1,?2,?3,?4,?5,datetime('now','localtime'))",params![name,size,fabric_id,quantity,sale_price]).map_err(|e|e.to_string())?;
  transaction.last_insert_rowid()
 };
 transaction.execute("INSERT INTO ready_productions(product_id,quantity,fabric_meters,tailor_name,created_at) VALUES(?1,?2,?3,?4,datetime('now','localtime'))",params![product_id,quantity,total_meters,tailor_name.trim()]).map_err(|e|e.to_string())?;
 let batch_id=transaction.last_insert_rowid();
 transaction.execute("INSERT INTO fabric_movements(fabric_id,movement_type,meters,balance_after,reference_type,reference_id,notes,created_at) VALUES(?1,'تفصيل جاهز',?2,?3,'تفصيل جاهز',?4,?5,datetime('now','localtime'))",params![fabric_id,-total_meters,balance,batch_id,format!("{} · {} · {} قطع",name,size,quantity)]).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;Ok(batch_id)
}

#[tauri::command]
fn list_extra_transactions(app:AppHandle)->Result<Vec<ExtraTransaction>,String>{
 let conn=db(&app)?;
 let mut statement=conn.prepare(
  "SELECT e.id,e.transaction_type,e.customer_name,e.customer_phone,e.fabric_id,
          COALESCE(f.name,''),COALESCE(f.color,''),e.quantity,e.meters,e.description,e.total_price,e.payment_method,e.worker_name,e.created_at,
          COALESCE(r.name||' · مقاس '||r.size,'')
   FROM extra_transactions e LEFT JOIN fabrics f ON f.id=e.fabric_id LEFT JOIN ready_products r ON r.id=e.ready_product_id ORDER BY e.id DESC LIMIT 100"
 ).map_err(|e|e.to_string())?;
 let rows=statement.query_map([],|row|Ok(ExtraTransaction{
  id:row.get(0)?,transaction_type:row.get(1)?,customer_name:row.get(2)?,customer_phone:row.get(3)?,
  fabric_id:row.get(4)?,fabric_name:row.get(5)?,fabric_color:row.get(6)?,quantity:row.get(7)?,meters:row.get(8)?,
  description:row.get(9)?,total_price:row.get(10)?,payment_method:row.get(11)?,worker_name:row.get(12)?,created_at:row.get(13)?,ready_product_name:row.get(14)?,
 })).map_err(|e|e.to_string())?;
 rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command]
fn save_extra_transaction(app:AppHandle,payload:ExtraTransactionPayload)->Result<ExtraTransaction,String>{
 let transaction_type=payload.transaction_type.trim();
 if transaction_type!="تصليح"&&transaction_type!="بيع قماش"&&transaction_type!="بيع جاهز"{return Err("نوع العملية غير صحيح".into())}
 if payload.customer_name.trim().is_empty(){return Err("اسم الزبون مطلوب".into())}
 if (transaction_type=="تصليح"||transaction_type=="بيع جاهز")&&payload.quantity<1{return Err("الكمية غير صحيحة".into())}
 if !payload.total_price.is_finite()||payload.total_price<0.0{return Err("السعر غير صحيح".into())}
 let mut conn=db(&app)?;
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 if transaction_type=="بيع قماش"{
  let fabric_id=payload.fabric_id.ok_or_else(||"اختر القماش المباع".to_string())?;
  if !payload.meters.is_finite()||payload.meters<=0.0{return Err("أدخل عدد الأمتار المباعة".into())}
  let stock:f64=transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=?1",[fabric_id],|row|row.get(0)).map_err(|_|"القماش غير موجود".to_string())?;
  if payload.meters>stock+0.0001{return Err(format!("المتوفر {:.2} متر فقط",stock))}
 }
 if transaction_type=="بيع جاهز"{
  let product_id=payload.ready_product_id.ok_or_else(||"اختر صنف الجاهز من المخزون".to_string())?;
  reserve_ready_product(&transaction,product_id,payload.quantity)?;
 }
 transaction.execute(
  "INSERT INTO extra_transactions(transaction_type,customer_name,customer_phone,fabric_id,quantity,meters,description,total_price,payment_method,worker_name,created_at,ready_product_id)
   VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,datetime('now','localtime'),?11)",
  params![transaction_type,payload.customer_name.trim(),payload.customer_phone.trim(),payload.fabric_id,payload.quantity.max(1),payload.meters.max(0.0),payload.description.trim(),payload.total_price,&payload.payment_method,payload.worker_name.trim(),if transaction_type=="بيع جاهز"{payload.ready_product_id}else{None}]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 if payload.total_price>0.0001{
  transaction.execute(
   "INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at,reference_type,reference_id) VALUES(?1,NULL,?2,?3,?4,datetime('now','localtime'),'دخل إضافي',?5)",
   params![transaction_type,format!("{} — {} — فاتورة {}",transaction_type,payload.customer_name.trim(),id),payload.total_price,&payload.payment_method,id]
  ).map_err(|e|e.to_string())?;
 }
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
          COALESCE(f.name,''),COALESCE(f.color,''),e.quantity,e.meters,e.description,e.total_price,e.payment_method,e.worker_name,e.created_at,
          COALESCE(r.name||' · مقاس '||r.size,'')
   FROM extra_transactions e LEFT JOIN fabrics f ON f.id=e.fabric_id LEFT JOIN ready_products r ON r.id=e.ready_product_id WHERE e.id=?1",
  [id],|row|Ok(ExtraTransaction{
   id:row.get(0)?,transaction_type:row.get(1)?,customer_name:row.get(2)?,customer_phone:row.get(3)?,
   fabric_id:row.get(4)?,fabric_name:row.get(5)?,fabric_color:row.get(6)?,quantity:row.get(7)?,meters:row.get(8)?,
   description:row.get(9)?,total_price:row.get(10)?,payment_method:row.get(11)?,worker_name:row.get(12)?,created_at:row.get(13)?,ready_product_name:row.get(14)?,
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
  if required>stock+0.0001&&!confirm_low_stock{return Err(format!("قماش {} — {}: المطلوب {:.2} متر والمتوفر {:.2} متر فقط. أكّد علمك بالنقص قبل المتابعة",name,color,required,stock))}
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

#[cfg(test)]
mod fabric_shortage_tests{
 use super::*;
 #[test]
 fn only_explicit_confirmation_allows_full_deduction_past_zero(){
  let mut conn=rusqlite::Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE fabrics(id INTEGER PRIMARY KEY,name TEXT,color TEXT,stock_meters REAL);CREATE TABLE invoice_fabric_usage(invoice_id INTEGER,fabric_id INTEGER,thobe_index INTEGER,meters REAL);CREATE TABLE fabric_movements(fabric_id INTEGER,movement_type TEXT,meters REAL,balance_after REAL,reference_type TEXT,reference_id INTEGER,notes TEXT,created_at TEXT);INSERT INTO fabrics(id,name,color,stock_meters) VALUES(1,'قطن','أبيض',1.0);").unwrap();
  let transaction=conn.transaction().unwrap();
  let usage=[FabricUsagePayload{fabric_id:1,thobe_index:0,meters:2.0}];
  assert!(apply_invoice_fabric_usage(&transaction,7,&usage,false).is_err());
  assert_eq!(transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=1",[],|row|row.get::<_,f64>(0)).unwrap(),1.0);
  apply_invoice_fabric_usage(&transaction,7,&usage,true).unwrap();
  assert_eq!(transaction.query_row("SELECT stock_meters FROM fabrics WHERE id=1",[],|row|row.get::<_,f64>(0)).unwrap(),-1.0);
  assert_eq!(transaction.query_row("SELECT meters FROM invoice_fabric_usage WHERE invoice_id=7",[],|row|row.get::<_,f64>(0)).unwrap(),2.0);
 }
}

fn apply_invoice_worker_cuts(transaction:&rusqlite::Transaction<'_>,invoice_id:i64,cuts:&[WorkerCutPayload])->Result<(),String>{
 let mut indexes=std::collections::HashSet::new();
 for cut in cuts{
  if cut.thobe_index<0||!indexes.insert(cut.thobe_index){return Err("رقم الثوب في أجور العمال غير صحيح أو مكرر".into())}
  let size=cut.thobe_size.trim();
  if size!="كبير"&&size!="صغير"{return Err("حجم الثوب في أجر العامل غير صحيح".into())}
 if !cut.amount.is_finite()||cut.amount<0.0{return Err("أجر العامل غير صحيح".into())}
 if cut.tailor_amount.is_some_and(|amount|!amount.is_finite()||amount<0.0){return Err("أجر الخياط غير صحيح".into())}
  let worker=cut.worker_name.trim();
  if !worker.is_empty(){
   let mode:String=transaction.query_row("SELECT pay_mode FROM workers WHERE name=?1 AND role='قصاص'",[worker],|row|row.get(0)).map_err(|_|"القصاص غير موجود".to_string())?;
   let existing:Option<(String,String,f64)>=transaction.query_row(
    "SELECT worker_name,thobe_size,amount FROM worker_cut_entries WHERE invoice_id=?1 AND thobe_index=?2",
    params![invoice_id,cut.thobe_index],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
   ).optional().map_err(|e|e.to_string())?;
   let earned=match existing{
    Some((old_worker,old_size,old_amount)) if old_worker==worker&&old_size==size=>old_amount,
    _ if mode=="قطعة"=>cut.amount,
    _=>0.0
   };
   transaction.execute(
    "INSERT INTO worker_cut_entries(invoice_id,thobe_index,worker_name,thobe_size,amount,created_at)
     VALUES(?1,?2,?3,?4,?5,datetime('now','localtime'))
     ON CONFLICT(invoice_id,thobe_index) DO UPDATE SET worker_name=excluded.worker_name,thobe_size=excluded.thobe_size,amount=excluded.amount",
    params![invoice_id,cut.thobe_index,worker,size,earned]
   ).map_err(|e|e.to_string())?;
  }
  if let Some(name)=cut.tailor_name.as_deref().map(str::trim).filter(|value|!value.is_empty()){
   let (worker_id,mode,large,small):(i64,String,f64,f64)=transaction.query_row(
    "SELECT id,pay_mode,large_rate,small_rate FROM workers WHERE name=?1 AND role='خياط'",[name],
    |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))
   ).map_err(|_|"الخياط غير موجود".to_string())?;
   let existing:Option<(i64,String,f64)>=transaction.query_row(
    "SELECT worker_id,thobe_size,amount FROM worker_tailor_entries WHERE invoice_id=?1 AND thobe_index=?2",
    params![invoice_id,cut.thobe_index],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
   ).optional().map_err(|e|e.to_string())?;
   let earned=if mode=="راتب"{0.0}else if let Some(custom_amount)=cut.tailor_amount{custom_amount}else{match existing{
    Some((old_id,old_size,old_amount)) if old_id==worker_id&&old_size==size=>old_amount,
    _=>if size=="كبير"{large}else{small}
   }};
   transaction.execute(
    "INSERT INTO worker_tailor_entries(invoice_id,thobe_index,worker_id,thobe_size,amount,created_at)
     VALUES(?1,?2,?3,?4,?5,datetime('now','localtime'))
     ON CONFLICT(invoice_id,thobe_index) DO UPDATE SET worker_id=excluded.worker_id,thobe_size=excluded.thobe_size,amount=excluded.amount",
    params![invoice_id,cut.thobe_index,worker_id,size,earned]
   ).map_err(|e|e.to_string())?;
  }
 }
 let placeholders=cuts.iter().map(|cut|cut.thobe_index.to_string()).collect::<Vec<_>>().join(",");
 if placeholders.is_empty(){
  transaction.execute("DELETE FROM worker_cut_entries WHERE invoice_id=?1",[invoice_id]).map_err(|e|e.to_string())?;
  transaction.execute("DELETE FROM worker_tailor_entries WHERE invoice_id=?1",[invoice_id]).map_err(|e|e.to_string())?;
 }else{
  transaction.execute(&format!("DELETE FROM worker_cut_entries WHERE invoice_id=?1 AND thobe_index NOT IN ({placeholders})"),[invoice_id]).map_err(|e|e.to_string())?;
  transaction.execute(&format!("DELETE FROM worker_tailor_entries WHERE invoice_id=?1 AND thobe_index NOT IN ({placeholders})"),[invoice_id]).map_err(|e|e.to_string())?;
  for cut in cuts{
   if cut.worker_name.trim().is_empty(){transaction.execute("DELETE FROM worker_cut_entries WHERE invoice_id=?1 AND thobe_index=?2",params![invoice_id,cut.thobe_index]).map_err(|e|e.to_string())?;}
   if cut.tailor_name.as_deref().unwrap_or("").trim().is_empty(){transaction.execute("DELETE FROM worker_tailor_entries WHERE invoice_id=?1 AND thobe_index=?2",params![invoice_id,cut.thobe_index]).map_err(|e|e.to_string())?;}
  }
 }
 Ok(())
}

fn validate_invoice_fabrics(details_json:&str,total_thobes:i64)->Result<(),String>{
 let details:serde_json::Value=serde_json::from_str(details_json).map_err(|_|"بيانات الثياب غير صالحة".to_string())?;
 let thobes=details.get("thobes").and_then(serde_json::Value::as_array).ok_or_else(||"بيانات الثياب غير مكتملة".to_string())?;
 if total_thobes<1||thobes.len()!=total_thobes as usize{return Err("عدد الثياب في الفاتورة غير صحيح".into())}
 for (index,thobe) in thobes.iter().enumerate(){
  let name=thobe.get("fabric").and_then(|fabric|fabric.get("اسم القماش")).and_then(serde_json::Value::as_str).unwrap_or("").trim();
  let fabric_id=thobe.get("fabricItemId").and_then(serde_json::Value::as_i64).unwrap_or(0);
  if !((fabric_id>0&&!name.is_empty())||(fabric_id==0&&name=="قماش العميل")){
   return Err(format!("اختر القماش للثوب {} قبل حفظ الفاتورة",index+1));
  }
  if thobe.get("tailorName").and_then(serde_json::Value::as_str).unwrap_or("").trim().is_empty(){
   return Err(format!("اختر الخياط للثوب {} قبل حفظ الفاتورة",index+1));
  }
 }
 Ok(())
}

#[cfg(test)]
mod invoice_fabric_tests{
 use super::validate_invoice_fabrics;
 #[test]
 fn every_thobe_requires_explicit_fabric(){
  let complete=r#"{"thobes":[{"fabricItemId":12,"fabric":{"اسم القماش":"قطن"},"tailorName":"محمد"},{"fabricItemId":null,"fabric":{"اسم القماش":"قماش العميل"},"tailorName":"علي"}]}"#;
  let incomplete=r#"{"thobes":[{"fabricItemId":12,"fabric":{"اسم القماش":"قطن"},"tailorName":"محمد"},{"fabricItemId":null,"fabric":{"اسم القماش":""},"tailorName":"علي"}]}"#;
  let no_tailor=r#"{"thobes":[{"fabricItemId":null,"fabric":{"اسم القماش":"قماش العميل"},"tailorName":""}]}"#;
  assert!(validate_invoice_fabrics(complete,2).is_ok());
  assert!(validate_invoice_fabrics(incomplete,2).unwrap_err().contains("للثوب 2"));
  assert!(validate_invoice_fabrics(no_tailor,1).unwrap_err().contains("الخياط"));
  assert!(validate_invoice_fabrics(complete,3).is_err());
 }
}

#[tauri::command]
fn save_invoice(app:AppHandle,payload:InvoicePayload)->Result<InvoiceRecord,String>{
 validate_invoice_fabrics(&payload.details_json,payload.total_thobes)?;
 if let Some(minimum)=payload.minimum_price{
  if !minimum.is_finite()||minimum<0.0{return Err("الحد الأدنى للسعر غير صالح".into())}
  let total=parse_money(&payload.total_price);let discount=parse_money(&payload.discount);
  if !total.is_finite()||!discount.is_finite()||total<0.0||discount<0.0||total+0.0001<minimum||total-discount+0.0001<minimum{return Err(format!("السعر غير مناسب: الإجمالي بعد الخصم لا يمكن أن يقل عن {} ر.س",minimum))}
 }
 let mut conn=db(&app)?;
 if let Some(id)=payload.id{
  if payload.new_customer.is_some(){return Err("لا يمكن إنشاء عميل جديد عند تعديل فاتورة".into())}
  let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let old_paid_text:String=transaction.query_row("SELECT paid_amount FROM invoices WHERE id=?1 AND customer_id=?2",params![id,payload.customer_id],|row|row.get(0)).map_err(|_|"الفاتورة غير موجودة".to_string())?;
  let payment_delta=parse_money(&payload.paid_amount)-parse_money(&old_paid_text);
  if payload.payment_splits.is_some()&&payment_delta<=0.0{return Err("قسّم الدفعة الإضافية فقط، أو سجّلها من قسم المالية".into())}
  let split_parts=if payment_delta>0.0{payment_parts(payment_delta,&payload.payment_method,payload.payment_splits.as_deref())?}else{Vec::new()};
  transaction.execute(
   "UPDATE invoices SET weight=?1,delivery_date=?2,day_count=?3,total_thobes=?4,total_price=?5,paid_amount=?6,payment_method=?7,discount=?8,notes=?9,measurements_json=?10,fabric_json=?11,designs_json=?12,details_json=?13,updated_at=datetime('now','localtime') WHERE id=?14 AND customer_id=?15",
   params![&payload.weight,&payload.delivery_date,payload.day_count,payload.total_thobes,&payload.total_price,&payload.paid_amount,&payload.payment_method,&payload.discount,&payload.notes,&payload.measurements_json,&payload.fabric_json,&payload.designs_json,&payload.details_json,id,payload.customer_id]
  ).map_err(|e|e.to_string())?;
  let (invoice_number,created_at,order_id):(String,String,i64)=transaction.query_row(
   "SELECT invoice_number,created_at,order_id FROM invoices WHERE id=?1 AND customer_id=?2",
   params![id,payload.customer_id],
   |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))
  ).map_err(|e|e.to_string())?;
  if payment_delta>0.0001{
   for (method,amount) in split_parts{
    transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![id,format!("تعديل دفعة الفاتورة {}",invoice_number),amount,method]).map_err(|e|e.to_string())?;
   }
  }else if payment_delta < -0.0001{
   transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![id,format!("تعديل دفعة الفاتورة {}",invoice_number),payment_delta,&payload.payment_method]).map_err(|e|e.to_string())?;
  }
  let effective_method=invoice_payment_summary(&transaction,id)?.unwrap_or_else(||payload.payment_method.clone());
  transaction.execute("UPDATE invoices SET payment_method=?1 WHERE id=?2",params![&effective_method,id]).map_err(|e|e.to_string())?;
  transaction.execute("UPDATE orders SET quantity=?1,delivery_date=NULLIF(?2,'') WHERE id=?3",params![payload.total_thobes,&payload.delivery_date,order_id]).map_err(|e|e.to_string())?;
  apply_invoice_fabric_usage(&transaction,id,&payload.fabric_usages,payload.confirm_low_stock)?;
  apply_invoice_worker_cuts(&transaction,id,&payload.worker_cuts)?;
  transaction.commit().map_err(|e|e.to_string())?;
  return Ok(InvoiceRecord{id,customer_id:payload.customer_id,invoice_number,created_at,payment_method:effective_method})
 }
 let transaction=conn.transaction().map_err(|e|e.to_string())?;
 let customer_id=if let Some(new_customer)=&payload.new_customer{
  let name=new_customer.name.trim();let phone=new_customer.phone.trim();
  if name.is_empty(){return Err("اسم العميل مطلوب".into())}
  if normalized_customer_phone(phone).len()<7{return Err("رقم الجوال غير صحيح".into())}
  ensure_customer_phone_available(&transaction,phone,None)?;
  transaction.execute("INSERT INTO customers(customer_code,name,phone,created_at) VALUES(NULL,?1,?2,datetime('now','localtime'))",params![name,phone]).map_err(|e|e.to_string())?;
  let id=transaction.last_insert_rowid();
  transaction.execute("UPDATE customers SET customer_code=?1 WHERE id=?2",params![id.to_string(),id]).map_err(|e|e.to_string())?;
  id
 }else{
  transaction.query_row("SELECT id FROM customers WHERE id=?1",[payload.customer_id],|row|row.get(0)).map_err(|_|"العميل غير موجود".to_string())?
 };
 transaction.execute(
  "INSERT INTO orders(received_date,quantity,print_status,delivery_date) VALUES(date('now','localtime'),?1,'بانتظار الطباعة',NULLIF(?2,''))",
  params![payload.total_thobes,&payload.delivery_date]
 ).map_err(|e|e.to_string())?;
 let order_id=transaction.last_insert_rowid();
 transaction.execute(
  "INSERT INTO invoices(invoice_number,order_id,customer_id,created_at,updated_at,weight,delivery_date,day_count,total_thobes,total_price,paid_amount,payment_method,discount,notes,measurements_json,fabric_json,designs_json,details_json) VALUES(NULL,?1,?2,datetime('now','localtime'),datetime('now','localtime'),?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
  params![order_id,customer_id,&payload.weight,&payload.delivery_date,payload.day_count,payload.total_thobes,&payload.total_price,&payload.paid_amount,&payload.payment_method,&payload.discount,&payload.notes,&payload.measurements_json,&payload.fabric_json,&payload.designs_json,&payload.details_json]
 ).map_err(|e|e.to_string())?;
 let id=transaction.last_insert_rowid();
 let invoice_number=id.to_string();
 transaction.execute("UPDATE invoices SET invoice_number=?1 WHERE id=?2",params![&invoice_number,id]).map_err(|e|e.to_string())?;
 let initial_payment=parse_money(&payload.paid_amount);
 let initial_parts=payment_parts(initial_payment,&payload.payment_method,payload.payment_splits.as_deref())?;
 for (method,amount) in initial_parts{
  transaction.execute("INSERT INTO financial_entries(entry_type,invoice_id,description,amount,payment_method,created_at) VALUES('دفعة فاتورة',?1,?2,?3,?4,datetime('now','localtime'))",params![id,format!("دفعة أولية للفاتورة {}",invoice_number),amount,method]).map_err(|e|e.to_string())?;
 }
 let effective_method=invoice_payment_summary(&transaction,id)?.unwrap_or_else(||payload.payment_method.clone());
 transaction.execute("UPDATE invoices SET payment_method=?1 WHERE id=?2",params![&effective_method,id]).map_err(|e|e.to_string())?;
 apply_invoice_fabric_usage(&transaction,id,&payload.fabric_usages,payload.confirm_low_stock)?;
 apply_invoice_worker_cuts(&transaction,id,&payload.worker_cuts)?;
 let created_at:String=transaction.query_row("SELECT created_at FROM invoices WHERE id=?1",[id],|row|row.get(0)).map_err(|e|e.to_string())?;
 transaction.commit().map_err(|e|e.to_string())?;
 Ok(InvoiceRecord{id,customer_id,invoice_number,created_at,payment_method:effective_method})
}

#[cfg(test)]
mod accounting_dashboard_report_tests{
 use super::*;

 #[test]
 fn blank_paid_and_discount_still_count_as_debt(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(id INTEGER PRIMARY KEY,customer_code TEXT,name TEXT,phone TEXT);CREATE TABLE invoices(id INTEGER PRIMARY KEY,customer_id INTEGER,created_at TEXT,total_price TEXT,paid_amount TEXT,discount TEXT);INSERT INTO customers VALUES(1,'1','عميل','0500000000');INSERT INTO invoices VALUES(1,1,'2026-09-19','300','','');INSERT INTO invoices VALUES(2,1,'2026-09-19','300','100','');INSERT INTO invoices VALUES(3,1,'2026-09-19','300','','20');INSERT INTO invoices VALUES(4,1,'2026-09-19','300','100','20');INSERT INTO invoices VALUES(5,1,'2026-09-19','','','');").unwrap();
  assert_eq!(total_customer_debt(&conn).unwrap(),960.0);
  let customer_debt:f64=conn.query_row(&format!("SELECT COALESCE(SUM({INVOICE_REMAINING_EXPRESSION}),0) FROM invoices i WHERE customer_id=1"),[],|row|row.get(0)).unwrap();
  assert_eq!(customer_debt,960.0);
 }

 #[test]
 fn dashboard_counts_each_event_and_direct_ready_transition(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE orders(id INTEGER PRIMARY KEY,received_date TEXT,tailored_date TEXT,quantity INTEGER,work_status TEXT,delivered_at TEXT);INSERT INTO orders VALUES(1,date('now','localtime'),NULL,2,'عند الخياط',NULL);INSERT INTO orders VALUES(2,date('now','localtime','-1 day'),NULL,3,'عند الخياط',NULL);INSERT INTO orders VALUES(3,date('now','localtime','-1 day'),date('now','localtime','-1 day'),1,'في المحل بانتظار التسليم',NULL);").unwrap();
  let before=dashboard_from_connection(&conn).unwrap();
  assert_eq!((before.received_today,before.tailored_today,before.ready_to_deliver),(2,0,1));
  update_order_stage(&conn,1,"في المغسلة").unwrap();
  update_order_stage(&conn,2,"في المحل بانتظار التسليم").unwrap();
  let after=dashboard_from_connection(&conn).unwrap();
  assert_eq!((after.received_today,after.tailored_today,after.ready_to_deliver),(2,5,4));
  update_order_stage(&conn,2,"عند الخياط").unwrap();
  let returned=dashboard_from_connection(&conn).unwrap();
  assert_eq!((returned.tailored_today,returned.ready_to_deliver),(2,1));
  update_order_stage(&conn,1,"في المحل بانتظار التسليم").unwrap();
  update_order_stage(&conn,1,"تم التسليم").unwrap();
  let delivered=dashboard_from_connection(&conn).unwrap();
  assert_eq!((delivered.tailored_today,delivered.ready_to_deliver),(2,1));
 }

 #[test]
 fn custom_report_range_includes_both_dates_and_excludes_neighbors(){
  let conn=Connection::open_in_memory().unwrap();
  conn.execute_batch("CREATE TABLE customers(created_at TEXT);CREATE TABLE invoices(created_at TEXT,total_thobes INTEGER,total_price TEXT);CREATE TABLE financial_entries(created_at TEXT,entry_type TEXT,amount REAL);CREATE TABLE extra_transactions(created_at TEXT,total_price REAL);CREATE TABLE orders(work_status TEXT,delivered_at TEXT,quantity INTEGER);CREATE TABLE fabric_movements(created_at TEXT,movement_type TEXT,meters REAL);INSERT INTO customers VALUES('2026-09-03 09:00:00'),('2026-09-20 00:00:00');INSERT INTO invoices VALUES('2026-09-03 10:00:00',2,'300'),('2026-09-19 23:59:59',1,'100'),('2026-09-20 00:00:00',1,'800');INSERT INTO financial_entries VALUES('2026-09-19 18:00:00','دفعة فاتورة',90),('2026-09-20 00:00:00','دفعة فاتورة',700);INSERT INTO extra_transactions VALUES('2026-09-19 18:00:00',15);INSERT INTO orders VALUES('تم التسليم','2026-09-19 22:00:00',2);").unwrap();
  let report=report_from_connection(&conn,"2026-09-03","2026-09-19","مخصص").unwrap();
  assert_eq!((report.new_customers,report.invoices,report.thobes,report.delivered),(1,2,3,2));
  assert_eq!((report.invoice_sales,report.invoice_received,report.extra_income),(400.0,90.0,15.0));
  assert_eq!(report.end_date,"2026-09-19");
  assert!(report_from_connection(&conn,"2026-09-20","2026-09-03","مخصص").is_err());
  assert!(report_from_connection(&conn,"2026-09-03","2026-09-31","مخصص").is_err());
 }
}

fn main(){
 let handler:fn(tauri::ipc::Invoke)->bool=tauri::generate_handler![license_status,activate_license,deactivate_license,print_direct,list_printers,report_printer_ready,save_auto_report_settings,dashboard_summary,work_board,delivery_payment_context,complete_delivery,advance_order_status,retreat_order_status,move_orders_to_status,current_session,session_history,storage_info,set_storage_location,create_customer,update_customer,delete_customer,delete_invoice,search_customers,find_customer_by_phone,customer_invoices,list_design_options,add_design_option,delete_design_option,list_suppliers,add_supplier,list_supplier_payments,add_supplier_payment,list_supplier_ledger,update_supplier_ledger_date,list_fabrics,add_fabric,restock_fabric,fabric_movements,list_ready_products,list_ready_productions,add_ready_production,list_ready_items,add_ready_item,delete_ready_item,list_notes,add_note,toggle_note,delete_note,list_whatsapp_campaigns,save_whatsapp_campaign,financial_overview,add_financial_entry,record_invoice_payment,daily_report,get_app_settings,save_app_settings,save_cut_prices,list_worker_names,list_workers,save_worker,add_worker,delete_worker,archive_worker,worker_account,record_worker_withdrawal,worker_ledger,clear_finance_pin,clear_app_pin,verify_finance_pin,verify_app_pin,admin_reset_pin,save_theme,list_extra_transactions,save_extra_transaction,save_invoice];
 tauri::Builder::default()
  .plugin(tauri_plugin_dialog::init())
  .setup(|app|{
   let session_id=if license::activated(&app.handle()).map_err(std::io::Error::other)?{
    Some(begin_session(&app.handle()).map_err(std::io::Error::other)?)
   }else{None};
   app.manage(SessionState(Mutex::new(session_id)));
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
  .invoke_handler(move |invoke|{
   let command=invoke.message.command();
   if command!="license_status"&&command!="activate_license"{
    match license::activated(&invoke.message.webview().app_handle()){
     Ok(true)=>{},
     Ok(false)=>{invoke.resolver.reject("يلزم تفعيل البرنامج لهذا الجهاز أولًا");return true},
     Err(error)=>{invoke.resolver.reject(error);return true},
    }
   }
   handler(invoke)
  })
  .run(tauri::generate_context!())
  .expect("تعذر تشغيل TAILOR");
}
