import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Dashboard = { receivedToday:number; awaitingPrint:number };

const arabicDate = new Intl.DateTimeFormat("ar-SA",{weekday:"long",day:"numeric",month:"long",year:"numeric"}).format(new Date());

function CustomerPlusIcon(){
  return <svg viewBox="0 0 72 72" aria-hidden="true">
    <circle cx="27" cy="22" r="10" />
    <path d="M10 51c1-11 8-17 17-17s16 6 17 17" />
    <path className="plus" d="M53 38v18M44 47h18" />
  </svg>;
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
    <header className="topbar"><div className="brand"><span>ت</span><div><strong>TAILOR</strong><small>إدارة التفصيل</small></div></div><time>{arabicDate}</time></header>
    <section className="welcome"><span>لوحة الرئيسية</span><h1>صباح العمل</h1><p>متابعة سريعة لما دخل المحل اليوم وما ينتظر طباعة المقاسات.</p></section>
    <section className="summary-grid" aria-live="polite">
      <article className="summary-card received"><div className="card-icon">↓</div><div><span>الثياب المستلمة اليوم</span><strong>{data?.receivedToday??"—"}</strong><small>ثوب سُجّل اليوم للتفصيل</small></div></article>
      <article className="summary-card printing"><div className="card-icon">▣</div><div><span>بانتظار طباعة المقاسات</span><strong>{data?.awaitingPrint??"—"}</strong><small>ثوب محفوظ ولم تُطبع ورقته بعد</small></div></article>
    </section>
    <section className="menu-section" aria-label="القائمة">
      <div className="section-heading"><span>القائمة</span><h2>ابدأ العمل</h2></div>
      <div className="menu-grid">
        <button className="menu-card" type="button" aria-label="إضافة عميل">
          <span className="menu-icon"><CustomerPlusIcon /></span>
          <span><b>إضافة عميل</b><small>تسجيل عميل جديد</small></span>
          <i>+</i>
        </button>
      </div>
    </section>
    <section className="note"><div><b>كيف تُحسب الأرقام؟</b><p>كل طلب يُسجّل اليوم يزيد «الثياب المستلمة اليوم». وبعد حفظ المقاسات يظهر في «بانتظار الطباعة» حتى تُعتمد طباعة ورقته.</p></div><button onClick={load}>تحديث الأرقام</button></section>
    {error&&<p className="error">{error}</p>}
  </main>;
}