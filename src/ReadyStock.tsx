import { useState, type FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ReadyItem = {
  id:number;quantity:number;initialQuantity:number;lengthInches:number;widthInches:number;
  unit:string;measurementsJson:string;designsJson:string;detailsJson:string;
  thobeType:string;fabricId:number|null;fabricLabel:string;salePrice:number;createdAt:string
};
export type ReadyDesign = { id:number;category:string;name:string;imageData:string };
export type ReadyFabric = { id:number;name:string;color:string;fabricKind:string };

export const readyMeasurementFields=[
  "طول أمام","طول خلف","الكتف","مقاس الصدر","مقاس العرض","مقاس عند الجيوب",
  "مقاس الخطوة تحت","خبنة","طول اليد","سعة اليد من فوق","سعة اليد من الوسط",
  "سعة اليد من تحت","الخبنة","طول الكبك","عرض الكبك","محيط الرقبة",
  "مقاس القلاب الأول","مقاس القلاب الثاني","مقاس الرقبة السادة"
] as const;
const designCategories=["نوع الثوب","اليد السادة","الكبك","القلاب","الرقبة السادة","الجيب","السحب","الجنب وأسفل الثوب","التطريز","الأزرار","الجنزور والتخليص"];
const numberFormat=new Intl.NumberFormat("ar-SA",{maximumFractionDigits:2});
const normalizeDigits=(value:string)=>value.replace(/[٠-٩]/g,d=>String("٠١٢٣٤٥٦٧٨٩".indexOf(d))).replace(/[۰-۹]/g,d=>String("۰۱۲۳۴۵۶۷۸۹".indexOf(d))).replace(/,/g,".");
const parseMeasure=(value:string)=>Number(normalizeDigits(value));
function readObject(json:string):Record<string,string|number>{
  try{const value=JSON.parse(json);return value&&typeof value==="object"&&!Array.isArray(value)?value:{}}catch{return {}}
}
export function ReadyCards({items,designs,onSell,onDelete}:{items:ReadyItem[];designs:ReadyDesign[];onSell?:(item:ReadyItem)=>void;onDelete?:(item:ReadyItem)=>void}){
  const [query,setQuery]=useState("");
  const [expanded,setExpanded]=useState<number|null>(null);
  const length=query.trim()?parseMeasure(query):null;
  const filtered=items.filter(item=>length===null||Number.isFinite(length)&&Math.abs(item.lengthInches-length)<=1.00001);
  return <section className="ready-browser" dir="rtl">
    <div className="ready-browser-head"><label>ابحث بالطول بالإنش ± إنش واحد
      <input className="numeric" inputMode="decimal" value={query} onChange={event=>setQuery(normalizeDigits(event.target.value))} placeholder="مثال: 58 يعرض 57 إلى 59"/>
    </label><strong>المتوفر: {numberFormat.format(filtered.reduce((sum,item)=>sum+item.quantity,0))} ثوب</strong></div>
    <div className="ready-grid">{filtered.map(item=>{const measurements=readObject(item.measurementsJson),selected=readObject(item.designsJson),details=readObject(item.detailsJson);return <article className="ready-card" key={item.id}>
      <header><b>جاهز · {item.thobeType}</b><small>متوفر {numberFormat.format(item.quantity)}</small></header>
      <div className="ready-card-numbers"><span>الطول <strong className="numeric">{numberFormat.format(item.lengthInches)} إنش</strong></span><span>العرض <strong className="numeric">{numberFormat.format(item.widthInches)} إنش</strong></span></div>
      <p>{item.fabricLabel} · سعر القطعة <b className="numeric">{numberFormat.format(item.salePrice)} ر.س</b></p>
      <div className="ready-card-actions"><button type="button" onClick={()=>setExpanded(current=>current===item.id?null:item.id)}>{expanded===item.id?"إخفاء المقاسات":"جميع المقاسات والصور"}</button>{onSell&&<button type="button" onClick={()=>onSell(item)}>بيع هذا المقاس</button>}{onDelete&&item.measurementsJson!=="{}"&&item.quantity===item.initialQuantity&&<button type="button" onClick={()=>onDelete(item)}>حذف</button>}</div>
      {expanded===item.id&&<div className="ready-card-details"><p>الوحدة الأصلية: {item.unit} · نوع اليد: {details.sleeveMode||"—"} · الرقبة: {details.collarMode||"—"}</p><div className="ready-measure-grid">{readyMeasurementFields.map(field=><span key={field}>{field}: <b className="numeric">{measurements[field]||"—"}</b></span>)}</div><div className="ready-photo-grid">{Object.entries(selected).map(([category,id])=>{const design=designs.find(option=>option.id===Number(id));return design&&<figure key={category}><img src={design.imageData} alt={design.name}/><figcaption>{category}: {design.name}</figcaption></figure>})}</div><p>{details.notes||""}</p></div>}
    </article>})}{filtered.length===0&&<p className="empty-business">لا يوجد ثوب جاهز بهذا الطول ضمن إنش أقل أو أكثر.</p>}</div>
  </section>
}

export function ReadyStockPage({items,designs,fabrics,reload,onBack,onSell}:{items:ReadyItem[];designs:ReadyDesign[];fabrics:ReadyFabric[];reload:()=>Promise<void>;onBack:()=>void;onSell:(item:ReadyItem)=>void}){
  const [measurements,setMeasurements]=useState<Record<string,string>>({});
  const [selectedDesigns,setSelectedDesigns]=useState<Record<string,number>>({});
  const [unit,setUnit]=useState<"إنش"|"سم">("إنش");
  const [thobeType,setThobeType]=useState("سعودي");
  const [sleeveMode,setSleeveMode]=useState("سادة");
  const [collarMode,setCollarMode]=useState("قلاب");
  const [fabricId,setFabricId]=useState("");
  const [fabricMeters,setFabricMeters]=useState("");
  const [quantity,setQuantity]=useState("1");
  const [price,setPrice]=useState("");
  const [notes,setNotes]=useState("");
  const [message,setMessage]=useState("");
  const [open,setOpen]=useState(false);
  async function save(event:FormEvent<HTMLFormElement>){
    event.preventDefault();
    try{
      setMessage("");
      await invoke<number>("add_ready_item",{payload:{
        quantity:Number(quantity),unit,measurementsJson:JSON.stringify(measurements),
        designsJson:JSON.stringify(selectedDesigns),
        detailsJson:JSON.stringify({sleeveMode,collarMode,notes}),
        thobeType,fabricId:fabricId?Number(fabricId):null,fabricMeters:parseMeasure(fabricMeters||"0"),salePrice:parseMeasure(price||"0")
      }});
      setMessage("تم تسجيل ثياب الجاهز بنجاح.");
      setMeasurements({});setSelectedDesigns({});setQuantity("1");setPrice("");setFabricMeters("");setNotes("");setOpen(false);
      await reload();
    }catch(error){setMessage(String(error))}
  }
  async function remove(item:ReadyItem){
    if(!window.confirm(`حذف مجموعة الجاهز بمقاس طول ${item.lengthInches} وعرض ${item.widthInches} وعدد ${item.quantity}؟`))return;
    try{await invoke("delete_ready_item",{id:item.id});await reload();setMessage("تم حذف المجموعة غير المباعة.")}catch(error){setMessage(String(error))}
  }
  return <section className="business-page ready-page" dir="rtl">
    <header className="business-head"><div><span>مخزون الثياب</span><h1>التفصيل الجاهز</h1><p>احفظ المقاسات والأشكال والكمية دون إنشاء عميل أو فاتورة. البيع يتم من «بيع جاهز».</p></div><button type="button" onClick={onBack}>رجوع للرئيسية</button></header>
    <button className="ready-add-toggle" type="button" onClick={()=>setOpen(value=>!value)}>{open?"إغلاق نموذج الجاهز":"إضافة تفصيل جاهز"}</button>
    {message&&<p className="business-message" role="alert">{message}</p>}
    {open&&<form className="business-form ready-form" onSubmit={event=>void save(event)}>
      <div className="form-pair"><label>نوع الثوب<select value={thobeType} onChange={event=>setThobeType(event.target.value)}>{["سعودي","قطري","كويتي","إماراتي"].map(type=><option key={type}>{type}</option>)}</select></label><label>وحدة المقاس<select value={unit} onChange={event=>setUnit(event.target.value as "إنش"|"سم")}><option>إنش</option><option>سم</option></select></label></div>
      <div className="ready-measure-grid">{readyMeasurementFields.map(field=><label key={field}><span>{field}</span><input className="numeric" inputMode="decimal" required={field==="طول أمام"||field==="مقاس العرض"} value={measurements[field]||""} onChange={event=>setMeasurements(current=>({...current,[field]:normalizeDigits(event.target.value)}))} onKeyDown={event=>{if(event.key!=="Enter")return;event.preventDefault();const grid=event.currentTarget.closest(".ready-measure-grid");const fields=Array.from(grid?.querySelectorAll<HTMLInputElement>("input")||[]);const next=fields[fields.indexOf(event.currentTarget)+1];if(next)next.focus();else grid?.nextElementSibling?.querySelector("select")?.focus()}}/></label>)}</div>
      <div className="form-pair"><label>نوع اليد<select value={sleeveMode} onChange={event=>setSleeveMode(event.target.value)}><option>سادة</option><option>كبك</option></select></label><label>نوع الرقبة<select value={collarMode} onChange={event=>setCollarMode(event.target.value)}><option>قلاب</option><option>سادة</option><option>بدون رقبة</option></select></label></div>
      <div className="ready-design-groups">{designCategories.map(category=>{const options=designs.filter(option=>option.category===category);return <section key={category}><h3>{category}</h3><div className="ready-photo-grid">{options.map(option=><button className={selectedDesigns[category]===option.id?"selected":""} type="button" key={option.id} onClick={()=>setSelectedDesigns(current=>({...current,[category]:current[category]===option.id?0:option.id}))}><img src={option.imageData} alt={option.name}/><span>{option.name}</span></button>)}{options.length===0&&<small>تُضاف الصور من مكتبة الأشكال في الإعدادات.</small>}</div></section>})}</div>
      <div className="form-pair"><label>القماش<select required value={fabricId} onChange={event=>setFabricId(event.target.value)}><option value="">اختر القماش من المخزون</option>{fabrics.map(item=><option key={item.id} value={item.id}>{item.fabricKind==="ملون"?"كاتلوج":"قماش"} {item.name} — {item.color}</option>)}</select></label><label>عدد الثياب<input className="numeric" type="number" min="1" required value={quantity} onChange={event=>setQuantity(event.target.value)}/></label></div>
      <label>استهلاك القماش لكل ثوب بالمتر<input className="numeric" inputMode="decimal" required value={fabricMeters} onChange={event=>setFabricMeters(normalizeDigits(event.target.value))}/><small>يُخصم كامل الاستهلاك عند إضافة الجاهز، ويُسترجع إذا حذفت المجموعة قبل بيعها.</small></label>
      <label>سعر بيع القطعة<input className="numeric" inputMode="decimal" required value={price} onChange={event=>setPrice(normalizeDigits(event.target.value))}/></label>
      <label>ملاحظة<textarea value={notes} onChange={event=>setNotes(event.target.value)}/></label>
      <button type="submit">إضافة إلى مخزون الجاهز</button>
    </form>}
    <ReadyCards items={items} designs={designs} onSell={onSell} onDelete={item=>void remove(item)}/>
  </section>
}
