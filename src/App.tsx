import { useState } from "react";
import "./App.css";

// ===== صفحات التطبيق (كل زر في الشريط الجانبي يفتح صفحة) =====
type Page = "home" | "library" | "bottles" | "settings" | "logs";

const NAV: { id: Page; label: string; icon: string }[] = [
  { id: "home", label: "الرئيسية", icon: "🏠" },
  { id: "library", label: "مكتبة الألعاب", icon: "🎮" },
  { id: "bottles", label: "القوارير", icon: "🍷" },
  { id: "settings", label: "الإعدادات", icon: "⚙️" },
  { id: "logs", label: "السجل والتشخيص", icon: "📜" },
];

function App() {
  // "page" = الصفحة المفتوحة حالياً
  const [page, setPage] = useState<Page>("home");

  return (
    <div className="app" dir="rtl">
      {/* ===== الشريط الجانبي ===== */}
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-logo">جسر</span>
          <span className="brand-sub">ألعاب ويندوز على الماك</span>
        </div>

        <nav className="nav">
          {NAV.map((item) => (
            <button
              key={item.id}
              className={page === item.id ? "nav-item active" : "nav-item"}
              onClick={() => setPage(item.id)}
            >
              <span className="nav-icon">{item.icon}</span>
              <span>{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebar-footer">
          <span className="badge">نسخة تجريبية 0.1.0</span>
          <span className="muted small">مشروع مفتوح المصدر</span>
        </div>
      </aside>

      {/* ===== المحتوى ===== */}
      <main className="content">
        {page === "home" && <Home />}
        {page === "library" && (
          <EmptyPage
            icon="🎮"
            title="مكتبة الألعاب"
            note="من هنا تضيف ألعابك: من ستيم أو ملف تثبيت .exe — قريباً جداً."
          />
        )}
        {page === "bottles" && (
          <EmptyPage
            icon="🍷"
            title="القوارير"
            note="القارورة = بيئة ويندوز معزولة لكل لعبة، عشان ما تتعارض الألعاب مع بعض — قريباً."
          />
        )}
        {page === "settings" && (
          <EmptyPage
            icon="⚙️"
            title="الإعدادات"
            note="اللغة، طبقات الرسومات (DXMT / DXVK)، الخطوط العربية، وأداء الجهاز — قريباً."
          />
        )}
        {page === "logs" && (
          <EmptyPage
            icon="📜"
            title="السجل والتشخيص"
            note="كل شي يصير في التطبيق ينكتب هنا، وفيه زر ينسخ التقرير عشان ترسله — قريباً."
          />
        )}
      </main>
    </div>
  );
}

// ===== الصفحة الرئيسية =====
function Home() {
  return (
    <>
      <header className="page-header">
        <h1>أهلاً بك في جسر 👋</h1>
        <p className="muted">
          شغّل ألعاب ويندوز على ماك بضغطة وحدة — بدون تيرمنال وبدون إعدادات معقّدة.
        </p>
      </header>

      <section className="cards">
        <div className="card">
          <div className="card-top">
            <span className="card-icon">🧩</span>
            <div>
              <h2>المحرك (Wine)</h2>
              <p className="muted small">القلب اللي يشغّل برامج ويندوز</p>
            </div>
          </div>
          <div className="status">
            <span className="dot off" />
            <span>غير مثبت بعد</span>
          </div>
          <button className="btn primary" disabled>
            تثبيت المحرك
          </button>
        </div>

        <div className="card">
          <div className="card-top">
            <span className="card-icon">🎮</span>
            <div>
              <h2>مكتبتك</h2>
              <p className="muted small">الألعاب اللي أضفتها</p>
            </div>
          </div>
          <div className="status">
            <span className="dot off" />
            <span>فاضية — 0 لعبة</span>
          </div>
          <button className="btn" disabled>
            أضف لعبة
          </button>
        </div>

        <div className="card">
          <div className="card-top">
            <span className="card-icon">💻</span>
            <div>
              <h2>جهازك</h2>
              <p className="muted small">معلومات الماك والأداء</p>
            </div>
          </div>
          <div className="status">
            <span className="dot" />
            <span>تُكتشف تلقائياً</span>
          </div>
          <button className="btn" disabled>
            فحص الجهاز
          </button>
        </div>
      </section>

      <section className="steps">
        <h2>ابدأ في 3 خطوات</h2>
        <ol className="steps-list">
          <li>
            <span className="step-num">1</span>
            <div>
              <b>ثبّت المحرك</b>
              <p className="muted small">ننزّل Wine + طبقات الرسومات تلقائياً</p>
            </div>
          </li>
          <li>
            <span className="step-num">2</span>
            <div>
              <b>أضف لعبة</b>
              <p className="muted small">من مكتبة ستيم أو ملف تثبيت .exe</p>
            </div>
          </li>
          <li>
            <span className="step-num">3</span>
            <div>
              <b>اضغط العب</b>
              <p className="muted small">ضبط تلقائي لأفضل أداء على جهازك</p>
            </div>
          </li>
        </ol>
      </section>

      <footer className="foot muted small">
        💡 الأزرار لسه معطّلة — هذي أول شاشة (هيكل)، وبنشغّل كل زر خطوة بخطوة.
      </footer>
    </>
  );
}

// ===== صفحة فاضية (نموذج مؤقت) =====
function EmptyPage({
  icon,
  title,
  note,
}: {
  icon: string;
  title: string;
  note: string;
}) {
  return (
    <div className="empty">
      <span className="empty-icon">{icon}</span>
      <h1>{title}</h1>
      <p className="muted">{note}</p>
    </div>
  );
}

export default App;
