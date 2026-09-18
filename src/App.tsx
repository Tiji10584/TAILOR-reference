import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Dashboard = { receivedToday:number; awaitingPrint:number };

const arabicDate = new Intl.DateTimeFormat("ar-SA",{weekday:"long",day:"numeric",month:"long",year:"numeric"}).format(new Date());

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
    <section className="note"><div><b>كيف تُحسب الأرقام؟</b><p>كل طلب يُسجّل اليوم يزيد «الثياب المستلمة اليوم». وبعد حفظ المقاسات يظهر في «بانتظار الطباعة» حتى تُعتمد طباعة ورقته.</p></div><button onClick={load}>تحديث الأرقام</button></section>
    {error&&<p className="error">{error}</p>}
  </main>;
}