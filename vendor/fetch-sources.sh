#!/usr/bin/env bash
# fetch-sources.sh — جلب سورس المكوّنات المفتوحة بنفس إصدارات CrossOver 26.3.0
#
# لماذا هذا الملف موجود: CrossOver يبني منتجه من مكوّنات مفتوحة (Wine LGPL،
# DXVK zlib، DXMT، MoltenVK Apache-2.0، vkd3d-proton LGPL، msync LGPL).
# نحن نأخذ **نفس المكوّنات من نفس المصادر** — والفرق الوحيد أن ترخيصها
# LGPL/GPL/Apache يبقى داخل الشجرة (مطلوب قانونياً: LGPL يفرض إتاحة السورس،
# وApache/MIT يفرضان إبقاء إشعارات الحقوق). من غيرها يصير المستودع م radioactive.
#
# الاستخدام:
#   bash vendor/fetch-sources.sh            # تحميل فقط (بدون بناء)
#   bash vendor/fetch-sources.sh --build    # تحميل + بناء Wine/MoltenVK
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${ROOT}/vendor/src"
mkdir -p "${VENDOR}"

log() { printf '\033[1;34m[vendor]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[vendor]\033[0m %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "أداة مفقودة: $1"; }
need git
need curl

# name|repo|tag-or-commit|license-note
PINNED=(
  "wine|https://github.com/wine-mirror/wine|wine-11.0|LGPL-2.1"
  "dxvk|https://github.com/doitsujin/dxvk|v2.4.1|zlib"
  "dxmt|https://github.com/3Shain/dxmt|master|MIT"
  "moltenvk|https://github.com/KhronosGroup/MoltenVK|v1.2.10|Apache-2.0"
  "vkd3d-proton|https://github.com/HansKristian-Work/vkd3d-proton|v2.14|LGPL-2.1"
  "msync|https://github.com/marzent/wine-msync|master|LGPL-2.1"
  "sparkle|https://github.com/sparkle-project/Sparkle|2.4.1|MIT"
)

fetch() {
  local name="$1" repo="$2" ref="$3" note="$4"
  local dest="${VENDOR}/${name}"
  if [ -d "${dest}/.git" ]; then
    log "موجود مسبقاً: ${name} (${ref}) — تخطّي"
    return 0
  fi
  log "جلب ${name} @ ${ref}  (${note})"
  git clone --filter=blob:none --no-checkout "${repo}" "${dest}" >/dev/null 2>&1 ||
    die "فشل استنساخ ${repo}"
  git -C "${dest}" checkout --quiet "${ref}" || die "فشل checkout ${ref} في ${name}"
  # ترخيص المكوّن يجب أن يبقى داخل الشجرة — هذا شرط GPL/LGPL/Apache.
  if [ ! -f "${dest}/LICENSE" ] && [ ! -f "${dest}/COPYING" ] && [ ! -f "${dest}/LICENSE.txt" ]; then
    log "⚠️  ${name}: لا يوجد ملف ترخيص قياسي — راجع الترخيص يدوياً قبل النشر"
  fi
}

for entry in "${PINNED[@]}"; do
  IFS='|' read -r name repo ref note <<< "${entry}"
  fetch "${name}" "${repo}" "${ref}" "${note}"
done

cat > "${VENDOR}/SOURCES.md" <<'EOF'
# مصادر المكوّنات الخارجية

| المكوّن | المصدر | الإصدار | الترخيص | ملاحظة |
|---|---|---|---|---|
| Wine | wine-mirror/wine | wine-11.0 | LGPL-2.1 | نفس محرك CrossOver 26.3.0 |
| DXVK | doitsujin/dxvk | v2.4.1 | zlib | D3D9/10/11 → Vulkan |
| DXMT | 3Shain/dxmt | master | MIT | D3D11 → Vulkan |
| MoltenVK | KhronosGroup/MoltenVK | v1.2.10 | Apache-2.0 | Vulkan → Metal |
| vkd3d-proton | HansKristian-Work/vkd3d-proton | v2.14 | LGPL-2.1 | D3D12 → Vulkan |
| msync | marzent/wine-msync | master | LGPL-2.1 | تسريع مزامنة |
| Sparkle | sparkle-project/Sparkle | 2.4.1 | MIT | التحديث التلقائي |

**شرطRelease:** LGPL تفرض إتاحة السورس ⇒ شجرة `vendor/src` هي نفس ما نوزّعه.
لا تحذف ملفات `LICENSE`/`COPYING` من أي مكوّن.
EOF

log "تمّ. الملفات في ${VENDOR}"
log "التراخيص موجودة داخل كل شجرة (مطلوبة للنشر) — راجع ${VENDOR}/SOURCES.md"
