import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

type Dashboard = { receivedToday:number; tailoredToday:number; dueToday:number };

const arabicDate = new Intl.DateTimeFormat("ar-SA-u-ca-gregory",{
  weekday:"long", day:"numeric", month:"long", year:"numeric",
}).format(new Date());

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
    {label==="إضافة عميل"&&<b className="card-plus">+</b>}
  </div>;
}

export default function App(){
  const [data,setData]=useState<Dashboard|null>(null);
  const [error,setError]=useState("");

  async function load(){
    try{setError("");setData(await invoke<Dashboard>("dashboard_summary"));}
    catch{setError("تعذر قراءة بيانات المحل المحلية.");}
  }

  useEffect(()=>{void load();},[]);

  return <main className="app-shell">
    <header className="topbar">
      <div className="brand"><span>ت</span><div><strong>TAILOR</strong><small>إدارة التفصيل</small></div></div>
      <time>{arabicDate}</time>
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
          <time>{arabicDate}</time>
        </section>

        <section className="metric-grid" aria-live="polite">
          <article className="metric-card received"><span className="metric-icon">↓</span><p>الثياب المستلمة اليوم</p><strong>{data?.receivedToday??"—"}</strong><small>طلب دخل اليوم</small></article>
          <article className="metric-card tailored"><span className="metric-icon">✦</span><p>تم تفصيلها اليوم</p><strong>{data?.tailoredToday??"—"}</strong><small>ثوب اكتمل تفصيله اليوم</small></article>
          <article className="metric-card due"><span className="metric-icon">◷</span><p>تسليمات اليوم</p><strong>{data?.dueToday??"—"}</strong><small>ثوب موعد تسليمه اليوم</small></article>
        </section>

        {error&&<p className="error">{error}</p>}
      </section>

      <aside className="side-menu side-left">
        <p className="side-label">متابعة المحل</p>
        <div className="side-cards">{leftMenu.map(([label,icon])=><MenuCard key={label} label={label} icon={icon}/>)}</div>
      </aside>
    </div>
  </main>;
}