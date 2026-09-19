import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

type Dashboard = { receivedToday:number; tailoredToday:number; dueToday:number };
type CurrentSession = { startedAt:string };
type SessionEntry = { startedAt:string; endedAt:string };
type DisplaySession = { start:Date; end:Date; active:boolean; key:string };

const dateFormat = new Intl.DateTimeFormat("ar-SA-u-ca-gregory-nu-latn",{
  weekday:"long", day:"2-digit", month:"long", year:"numeric",
});
const hijriDateFormat = new Intl.DateTimeFormat("ar-SA-u-ca-islamic-umalqura-nu-latn",{
  weekday:"long", day:"2-digit", month:"long", year:"numeric",
});
const timeFormat = new Intl.DateTimeFormat("ar-SA-u-nu-latn",{hour:"2-digit",minute:"2-digit",hour12:false});
const numberFormat = new Intl.NumberFormat("en-US",{useGrouping:false});

function databaseDate(value:string){return new Date(value.replace(" ","T"));}
function dayKey(date:Date){
  return `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,"0")}-${String(date.getDate()).padStart(2,"0")}`;
}
function durationLabel(milliseconds:number){
  const totalMinutes=Math.max(0,Math.floor(milliseconds/60000));
  const hours=Math.floor(totalMinutes/60);
  const minutes=totalMinutes%60;
  if(hours===0)return `${numberFormat.format(minutes)} دقيقة`;
  return `${numberFormat.format(hours)} ساعة${minutes ? ` و ${numberFormat.format(minutes)} دقيقة` : ""}`;
}

type IconName = "person"|"search"|"whatsapp"|"finance"|"supplier"|"income"|"delivery"|"notes"|"report"|"access"|"inventory";

const rightMenu:[string,IconName][] = [
  ["إضافة عميل","person"],
  ["بحث عن عميل","search"],
  ["إعلانات واتساب","whatsapp"],
  ["المعاملات المالية","finance"],
  ["الموردون","supplier"],
];

const leftMenu:[string,IconName][] = [
  ["دخل إضافي","income"],
  ["توزيع الثياب","delivery"],
  ["ملاحظات جديدة","notes"],
  ["تقرير يومي","report"],
  ["الصلاحيات","access"],
  ["المخزون","inventory"],
];

function Icon({name}:{name:IconName}){
  const common={fill:"none",stroke:"currentColor",strokeWidth:1.8,strokeLinecap:"round" as const,strokeLinejoin:"round" as const};
  const paths:Record<IconName,ReactNode>={
    person:<><circle cx="10" cy="8" r="3"/><path d="M4 19c.8-3.4 2.8-5 6-5s5.2 1.6 6 5"/><path d="M18 9v6M15 12h6"/></>,
    search:<><circle cx="10" cy="10" r="5"/><path d="m14 14 5 5"/></>,
    whatsapp:<><path d="M18.5 10.5a8.5 8.5 0 0 1-10.7 8.2L4 20l1.3-3.6A8.5 8.5 0 1 1 18.5 10.5Z"/><path d="M8 8.3c.5 2.8 2.4 4.6 5.2 5.2l1.3-1.1 1.6.8c-.4 1.6-1.4 2-2.4 1.7-3.9-1.2-6.3-3.6-7.4-7.4-.3-1 .1-2 1.7-2.4l.8 1.6Z"/></>,
    finance:<><path d="M3.5 7.5h17v11h-17z"/><path d="M3.5 10h17M8 15h3"/></>,
    supplier:<><path d="M4 9h16v11H4z"/><path d="M7 9V6h10v3M8 14h8"/></>,
    income:<><path d="M4 16 9 11l3 3 7-8"/><path d="M14 6h5v5"/></>,
    delivery:<><path d="M3 8h11v9H3zM14 11h3l3 3v3h-6z"/><circle cx="7" cy="18" r="1.5"/><circle cx="17" cy="18" r="1.5"/></>,
    notes:<><path d="M5 3.5h14v17H5z"/><path d="M8 8h8M8 12h8M8 16h5"/></>,
    report:<><path d="M4 20V4"/><path d="M7 17v-5M12 17V8M17 17V5"/></>,
    access:<><path d="M12 3 19 6v5c0 4.2-2.6 7.3-7 10-4.4-2.7-7-5.8-7-10V6z"/><path d="M9.5 12 11 13.5l3.5-4"/></>,
    inventory:<><path d="m4 8 8-4 8 4-8 4zM4 8v8l8 4 8-4V8M12 12v8"/></>,
  };
  return <svg viewBox="0 0 24 24" aria-hidden="true" {...common}>{paths[name]}</svg>;
}

function MenuCard({label,icon}:{label:string;icon:IconName}){
  return <div className="nav-card" aria-disabled="true">
    <span className="nav-icon"><Icon name={icon}/></span>
    <span>{label}</span>
  </div>;
}

export default function App(){
  const [data,setData]=useState<Dashboard|null>(null);
  const [session,setSession]=useState<CurrentSession|null>(null);
  const [history,setHistory]=useState<SessionEntry[]>([]);
  const [historyOpen,setHistoryOpen]=useState(false);
  const [selectedDay,setSelectedDay]=useState("");
  const [now,setNow]=useState(Date.now());
  const [error,setError]=useState("");

  async function load(){
    try{
      setError("");
      const [dashboard,current]=await Promise.all([
        invoke<Dashboard>("dashboard_summary"),
        invoke<CurrentSession>("current_session"),
      ]);
      setData(dashboard);
      setSession(current);
    }catch{setError("تعذر قراءة بيانات المحل المحلية.");}
  }

  async function showHistory(){
    setHistoryOpen(true);
    try{setHistory(await invoke<SessionEntry[]>("session_history"));}
    catch{setError("تعذر قراءة سجل التشغيل.");}
  }

  useEffect(()=>{
    void load();
    const interval=window.setInterval(()=>setNow(Date.now()),1000);
    return ()=>window.clearInterval(interval);
  },[]);

  const today=new Date();
  const started=session ? databaseDate(session.startedAt) : null;
  const elapsed=started ? durationLabel(now-started.getTime()) : "—";
  const displayHistory=useMemo<DisplaySession[]>(()=>{
    const saved=history.map((item,index)=>({
      start:databaseDate(item.startedAt),
      end:databaseDate(item.endedAt),
      active:false,
      key:`${item.startedAt}-${index}`,
    }));
    if(started)saved.unshift({start:started,end:new Date(now),active:true,key:`active-${session?.startedAt}`});
    return saved;
  },[history,now,session?.startedAt,started]);
  const filteredHistory=selectedDay ? displayHistory.filter(item=>dayKey(item.start)===selectedDay) : displayHistory;

  return <main className="app-shell">
    <header className="topbar">
      <div className="brand"><span>ت</span><div><strong>TAILOR</strong><small>إدارة التفصيل</small></div></div>
      <time className="numeric">{dateFormat.format(today)}</time>
    </header>

    <div className="workspace">
      <aside className="side-menu side-right">
        <p className="side-label">القائمة الرئيسية</p>
        <div className="side-cards">{rightMenu.map(([label,icon])=><MenuCard key={label} label={label} icon={icon}/>)}</div>
      </aside>

      <section className="dashboard">
        <section className="greeting">
          <span>مساحة العمل</span>
          <h1>مرحباً بك</h1>
          <p>هذه نظرة اليوم على حركة التفصيل والتسليم.</p>
          <time className="numeric">{dateFormat.format(today)}</time>
        </section>

        <section className="metric-grid" aria-live="polite">
          <article className="metric-card received"><span className="metric-icon">↓</span><p>القبض</p><strong className="numeric">{data ? numberFormat.format(data.receivedToday) : "—"}</strong></article>
          <article className="metric-card tailored"><span className="metric-icon">✦</span><p>تم تفصيلها اليوم</p><strong className="numeric">{data ? numberFormat.format(data.tailoredToday) : "—"}</strong></article>
          <article className="metric-card due"><span className="metric-icon">◷</span><p>موعودين اليوم</p><strong className="numeric">{data ? numberFormat.format(data.dueToday) : "—"}</strong></article>
        </section>

        <button className="session-card" type="button" onClick={()=>void showHistory()}>
          <span className="session-kicker">سجل تشغيل التطبيق</span>
          <span className="session-dates">
            <strong className="numeric">{dateFormat.format(today)}</strong>
            <small className="numeric">هجريًا: {hijriDateFormat.format(today)}</small>
          </span>
          <span className="session-runtime">
            <small>فُتح التطبيق عند</small>
            <strong className="numeric">{started ? timeFormat.format(started) : "—"}</strong>
            <small>مدة التشغيل الحالية</small>
            <b className="numeric">{elapsed}</b>
          </span>
          <span className="session-action">عرض سجل الأيام ←</span>
        </button>

        {error&&<p className="error">{error}</p>}
      </section>

      <aside className="side-menu side-left">
        <p className="side-label">متابعة المحل</p>
        <div className="side-cards">{leftMenu.map(([label,icon])=><MenuCard key={label} label={label} icon={icon}/>)}</div>
      </aside>
    </div>

    {historyOpen&&<div className="history-overlay" role="presentation" onMouseDown={()=>setHistoryOpen(false)}>
      <section className="history-dialog" role="dialog" aria-modal="true" aria-labelledby="history-title" onMouseDown={event=>event.stopPropagation()}>
        <div className="history-head">
          <div><span>تشغيل التطبيق</span><h2 id="history-title">سجل أوقات التشغيل</h2></div>
          <label className="history-filter">
            <span>ابحث بالتاريخ الميلادي</span>
            <input className="numeric" type="date" lang="ar-SA-u-ca-gregory-nu-latn" value={selectedDay} onChange={event=>setSelectedDay(event.target.value)}/>
          </label>
          <button type="button" onClick={()=>setHistoryOpen(false)} aria-label="إغلاق">×</button>
        </div>
        <div className="history-summary">
          {selectedDay ? <>في <b className="numeric">{selectedDay}</b> فُتح التطبيق <strong className="numeric">{numberFormat.format(filteredHistory.length)}</strong> مرات</> : "اختر تاريخًا لعرض عدد مرات الفتح في ذلك اليوم."}
        </div>
        <div className="history-list">
          {filteredHistory.length===0&&<p>{selectedDay ? "لا توجد جلسات في التاريخ المختار." : "لا توجد جلسات مكتملة بعد."}</p>}
          {filteredHistory.map(item=><article className="history-row" key={item.key}>
            <div><strong className="numeric">{dateFormat.format(item.start)}</strong><small className="numeric">هجريًا: {hijriDateFormat.format(item.start)}</small></div>
            <div><span className="numeric">{timeFormat.format(item.start)} — {timeFormat.format(item.end)}</span><b className="numeric">{durationLabel(item.end.getTime()-item.start.getTime())}</b>{item.active&&<small>نشطة الآن</small>}</div>
          </article>)}
        </div>
      </section>
    </div>}
  </main>;
}