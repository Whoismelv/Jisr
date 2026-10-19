#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""جسر — تحديث الرسم المعرفي (graphify) بنهاية الجلسة.

الاستخدام (من جذر المستودع):
    python tools/graphify/session_end.py          # كشف التغييرات + تحديث جزء الكود (AST، بلا LLM)
    python tools/graphify/session_end.py --finish # دمج استخراج المستندات + إعادة التجميع النهائي

المستندات (PROJECT.md، README.md، docs/…) تحتاج استخراجاً دلالياً يقوم به المساعد
الذكي (subagent): يقرأ الملفات المتغيرة ويكتب الناتج في
    graphify-out/.graphify_chunk_docs_1.json  (وزّعها 1، 2، … عند تعدد الدفعات)
ثم شغّل `--finish` لإكمال الدمج في الرسم النهائي (graph.json + التقرير + HTML).

ملاحظة: بعد كل commit، يحدّث git-hook جزء الكود تلقائياً — هذا السكربت يغطي
التغييرات غير المُلتزَمة + المستندات + إعادة التوليد النهائي.

الخروج: 0 = لا شيء مطلوب — 2 = بانتظار استخراج مستندات (chunks) — 1/3/4 = أخطاء.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "graphify-out"
PENDING = OUT / ".session_pending_docs.json"
INCREMENTAL = OUT / ".graphify_incremental.json"

# نفس استثناءات البناء الكامل: مجلدات بناء/أيقونات لا تضيف معرفة
EXCLUDES = [
    "src-tauri/target/",
    "graphify-out/",
    "src-tauri/icons/",
    "public/",
    "src/assets/",
]
SEMANTIC_TYPES = ("document", "paper", "image", "video")

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass


def _fail(msg: str, code: int = 1) -> None:
    print(f"[session-end] ✗ {msg}")
    sys.exit(code)


def _rel(path_str: str) -> str:
    """مسار مطلق → نسبي لجذر المستودع (للعرض فقط)."""
    try:
        return str(Path(path_str).resolve().relative_to(REPO))
    except Exception:
        return path_str


def detect_and_update_code() -> int:
    os.environ.setdefault("PYTHONHASHSEED", "0")
    os.environ.setdefault("GRAPHIFY_MAX_WORKERS", "1")

    if not OUT.exists() or not (OUT / "graph.json").exists():
        print("[session-end] graphify-out غير موجود — الأداة غير مثبتة على هذا الجهاز، لا شيء مطلوب.")
        return 0

    from graphify.detect import detect_incremental
    from graphify.paths import out_path
    from graphify.watch import _read_build_excludes, _read_build_gitignore

    result = detect_incremental(
        REPO,
        extra_excludes=EXCLUDES + list(_read_build_excludes(out_path())),
        gitignore=_read_build_gitignore(out_path()),
    )
    INCREMENTAL.write_text(
        json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8"
    )

    nf = result.get("new_files", {}) or {}
    code = list(nf.get("code", []))
    sem = [f for t in SEMANTIC_TYPES for f in nf.get(t, [])]
    deleted = list(result.get("deleted_files", []) or [])
    new_total = result.get("new_total", 0)

    print(
        f"[session-end] تغييرات: {new_total} ملف جديد/متغير، {len(deleted)} محذوف "
        f"| كود: {len(code)} | مستندات: {len(sem)}"
    )

    if new_total == 0 and not deleted:
        print("[session-end] ✓ لا تغييرات — الرسم محدّث أصلاً.")
        INCREMENTAL.unlink(missing_ok=True)
        return 0

    if code or deleted:
        # المسار المُصان للـCLI: يستخرج الكود (AST)، يدمج، يشذّب المحذوف، ويولّد المخرجات
        proc = subprocess.run(
            [sys.executable, "-m", "graphify", "update", str(REPO)],
            cwd=str(REPO),
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        tail = (proc.stdout or "").strip().splitlines()[-3:]
        for line in tail:
            print("[session-end]   " + line)
        if proc.returncode != 0:
            print("[session-end] ✗ فشل تحديث الكود — راجع المخرجات فوق.")
            return 1

    if sem:
        PENDING.write_text(
            json.dumps({"docs": sem, "deleted": deleted}, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        covered: set[str] = set()
        for c in sorted(OUT.glob(".graphify_chunk_docs_*.json")):
            try:
                d = json.loads(c.read_text(encoding="utf-8"))
            except Exception:
                continue
            for side in ("nodes", "edges", "hyperedges"):
                for item in d.get(side, []) or []:
                    sf = item.get("source_file")
                    if sf:
                        covered.add(str(sf))
        missing = [f for f in sem if f not in covered]
        if missing:
            print("[session-end] مطلوب استخراج دلالي (subagent) للمستندات المتغيرة:")
            for f in missing:
                print("   - " + _rel(f))
            print(
                "[session-end]   اكتب الناتج في graphify-out/.graphify_chunk_docs_<N>.json "
                "ثم شغّل: python tools/graphify/session_end.py --finish"
            )
            return 2
        print("[session-end] chunks المستندات جاهزة — شغّل  --finish  لإكمال الدمج.")
        return 2

    print("[session-end] ✓ جزء الكود محدّث. لا مستندات متغيرة.")
    INCREMENTAL.unlink(missing_ok=True)
    PENDING.unlink(missing_ok=True)
    return 0


def finish() -> int:
    os.environ.setdefault("PYTHONHASHSEED", "0")
    os.environ.setdefault("GRAPHIFY_MAX_WORKERS", "1")

    if not PENDING.exists():
        _fail("لا يوجد .session_pending_docs.json — شغّل السكربت بدون --finish أولاً.", 0)

    pending = json.loads(PENDING.read_text(encoding="utf-8"))
    docs = list(pending.get("docs", []))
    deleted = list(pending.get("deleted", []))

    chunks = sorted(OUT.glob(".graphify_chunk_docs_*.json"))
    if not chunks:
        _fail("ما لقيت أي chunk — لازم الـsubagent يكتب graphify-out/.graphify_chunk_docs_*.json", 3)

    nodes: list = []
    edges: list = []
    hyper: list = []
    tok_in = 0
    tok_out = 0
    covered: set[str] = set()
    for c in chunks:
        d = json.loads(c.read_text(encoding="utf-8"))
        nodes += d.get("nodes", []) or []
        edges += d.get("edges", []) or []
        hyper += d.get("hyperedges", []) or []
        tok_in += int(d.get("input_tokens", 0) or 0)
        tok_out += int(d.get("output_tokens", 0) or 0)
        for side in ("nodes", "edges", "hyperedges"):
            for item in d.get(side, []) or []:
                sf = item.get("source_file")
                if sf:
                    covered.add(str(sf))

    missing = [f for f in docs if f not in covered]
    if missing:
        print("[session-end] ✗ الـchunks ما غطّت المستندات التالية:")
        for f in missing:
            print("   - " + _rel(f))
        return 3

    sem = {
        "nodes": nodes,
        "edges": edges,
        "hyperedges": hyper,
        "input_tokens": tok_in,
        "output_tokens": tok_out,
    }

    from graphify.build import build_merge
    from graphify.export import to_json
    from graphify.detect import save_manifest
    from graphify.cli import _stamped_manifest_files

    G = build_merge(
        [sem],
        graph_path=str(OUT / "graph.json"),
        prune_sources=(deleted or None),
        root=str(REPO),
        directed=False,
    )
    print(
        f"[session-end] دمج المستندات: {G.number_of_nodes()} عقدة، "
        f"{G.number_of_edges()} حافة — إعادة تجميع وتوليد المخرجات…"
    )

    # الكتابة بصيغة graph.json القياسية (communities مؤقتة من السمات؛ cluster-only سيعيد التجميع)
    communities: dict[int, list[str]] = {}
    for nid, attrs in G.nodes(data=True):
        cid = attrs.get("community")
        try:
            cid = int(cid) if cid is not None else -1
        except Exception:
            cid = -1
        communities.setdefault(cid, []).append(nid)
    labels = {}
    labels_file = OUT / ".graphify_labels.json"
    if labels_file.exists():
        try:
            raw = json.loads(labels_file.read_text(encoding="utf-8"))
            labels = {int(k): v for k, v in raw.items()}
        except Exception:
            labels = {}
    if not to_json(G, communities, str(OUT / "graph.json"), force=True, community_labels=labels):
        _fail("to_json رفض الكتابة — تحقق من graph.json", 1)

    proc = subprocess.run(
        [sys.executable, "-m", "graphify", "cluster-only", str(REPO)],
        cwd=str(REPO),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    for line in (proc.stdout or "").strip().splitlines()[-4:]:
        print("[session-end]   " + line)
    if proc.returncode != 0:
        print(
            "[session-end] ✗ فشلت إعادة التجميع — الرسم مدموج لكن التقرير/HTML قديم. "
            "أعد المحاولة: python -m graphify cluster-only ."
        )
        return 4

    # ختم المستندات في الـmanifest حتى لا تُعاد في التحديث القادم
    inc = {}
    if INCREMENTAL.exists():
        try:
            inc = json.loads(INCREMENTAL.read_text(encoding="utf-8"))
        except Exception:
            inc = {}
    corpus = inc.get("all_files") or inc.get("files") or {}
    if corpus:
        stamped = _stamped_manifest_files(corpus, sem, REPO)
        scan = {f for fl in corpus.values() for f in fl if f}
        cleared = set(docs) - {f for fl in stamped.values() for f in fl if f}
        save_manifest(stamped, root=REPO, scan_corpus=scan, clear_semantic=cleared or None)
        print("[session-end] ✓ حُدّث manifest المستندات.")
    else:
        print("[session-end] ⚠ لا يوجد .graphify_incremental.json — تخطّي ختم الـmanifest (سيتكرر الكشف لاحقاً).")

    # تنظيف ملفات العمل المؤقتة
    for c in chunks:
        c.unlink(missing_ok=True)
    PENDING.unlink(missing_ok=True)
    INCREMENTAL.unlink(missing_ok=True)

    print("[session-end] ✓ تم — الرسم والتقرير وHTML محدّثون.")
    return 0


def main() -> int:
    if "--finish" in sys.argv[1:]:
        return finish()
    return detect_and_update_code()


if __name__ == "__main__":
    sys.exit(main())
