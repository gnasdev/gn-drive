---
title: "GN Drive Frontend SPEC"
description: "Design contract cho Vue 3 SPA của gn-drive: design read, dial (variance/motion/density), pre-flight check matrix, và conventions để mọi PR frontend pass trước khi merge."
type: "module"
status: "active"
tags: ["frontend", "spec", "vue", "design", "taste"]
updated: "2026-06-23"
scope: "frontend/src/**/*.{vue,ts,css}"
source: "docs/specs/planning/refactor-gn-drive-web-stack.md §6"
compliance: "design-taste-frontend skill, image-to-code skill"
---

# GN Drive Frontend SPEC

> Mọi PR frontend **phải** pass Pre-Flight Check ở §4 trước khi merge.
> Design contract này mirror `docs/specs/planning/refactor-gn-drive-web-stack.md` §6 và bind vào `design-taste-frontend` skill.

## 1. Design Read

GN Drive frontend là **utility devtool SPA**, self-hosted, dùng bởi sysadmin / dev / power-user. Tone: **calm + dense**, dark mode mặc định, terminal-style. Tối ưu cho **data visibility** chứ không phải marketing wow factor.

**Reference vibe**: Linear, Tailscale admin, Vaultwarden, Syncthing GUI. KHÔNG phải SaaS landing page.

**Audience intent**:
- Mở app lên → thấy trạng thái sync hiện tại trong 2 giây.
- Thêm profile / remote / schedule mà không cần đọc doc.
- Tìm log + history dễ dàng, không phải click 4 menu.

**Layout pattern chuẩn**: 2-pane (sidebar + main content). Mỗi surface lặp lại pattern `left-list ↔ right-detail`.

## 2. Dial Configuration

| Dial | Giá trị | Lý do |
| --- | --- | --- |
| `DESIGN_VARIANCE` | **4** | 2-pane layout, log panel thu gọn, không có experimental. |
| `MOTION_INTENSITY` | **2** | Utility devtool: chỉ micro-tactile, skeleton loader, KHÔNG scroll-hijack, KHÔNG parallax. |
| `VISUAL_DENSITY` | **7** | Dashboard hiển thị nhiều data (profile list, task list, log stream); tight spacing. |

Lưu tại `frontend/SPEC.md` (file này). Cả team tham chiếu khi review.

## 3. Conventions (binding)

### 3.1 Typography
- Font: `@fontsource-variable/geist` (đã có sẵn, self-host).
- Cấm `Inter`, cấm Google Fonts.
- Cấm em-dash (`—`) trong UI text. Dùng `:`, `,`, hoặc rephrase.
- Tabular numbers: `font-variant-numeric: tabular-nums` cho mọi stat value.

### 3.2 Color
- Theme tokens qua CSS custom properties trong `src/styles/main.css`, scope `:root` (dark default) và `:root.light`.
- Tailwind v4 `@theme inline` map `--color-*` → utility (`bg-bg`, `text-muted`, `border-border`).
- KHÔNG hardcode hex inline trong component. Dùng token.
- Status colors: `--color-success`, `--color-danger`, `--color-warning` — KHÔNG pha màu riêng.

### 3.3 Spacing
- Base unit: 4px. Dùng Tailwind `p-2`, `p-4`, `gap-3`, `gap-4`.
- Cấm random `style="padding: 13px"`.
- Sidebar 240px, Topbar 48px (cố định).

### 3.4 Motion
- Chỉ micro-tactile: hover state, focus ring, 150ms ease-out.
- Skeleton loader cho async data (dùng `animate-pulse` Tailwind).
- KHÔNG scroll-hijack, KHÔNG parallax, KHÔNG bouncy transitions.
- KHÔNG dùng `framer-motion` trừ khi có lý do rõ ràng (chưa có trong stack hiện tại).

### 3.5 Icons
- `@phosphor-icons/vue` (đã có sẵn). Dùng weight `regular` mặc định.
- KHÔNG trộn emoji + ảnh placeholder. KHÔNG dùng emoji làm icon.
- Icon size: 16px (inline), 20px (button), 24px (sidebar nav).

### 3.6 Forbidden
- ❌ em-dash trong UI text
- ❌ "Quietly trusted by" / "Used at" / "From the field" labels
- ❌ pills overlay trên ảnh
- ❌ three-equal feature cards (chỉ dùng 4-tile bento nếu cần, mỗi tile nội dung thật)
- ❌ Inter font, Google Fonts
- ❌ emoji làm icon
- ❌ shadcn-vue copy-paste (plan nói rõ dùng Reka UI primitives hoặc `@nuxt/ui` style composables)

## 4. Pre-Flight Check (binding — required trước merge)

Mỗi PR frontend phải đối chiếu matrix này. Tick mỗi hạng mục.

### 4.1 Spacing
- [ ] Sidebar width = 240px cố định.
- [ ] Topbar height = 48px cố định.
- [ ] Page padding = `p-6` (24px) hoặc `p-8` (32px), không random.
- [ ] Row gap trong list = `gap-3` (12px) hoặc `gap-4` (16px).
- [ ] Card padding = `p-4` (16px).

### 4.2 Typography
- [ ] Font family = Geist (không Inter, không system).
- [ ] Stat values dùng `font-variant-numeric: tabular-nums`.
- [ ] Heading: `text-lg font-semibold` (page title), `text-sm font-medium uppercase tracking-wide` (section).
- [ ] Body: `text-sm` (14px) hoặc `text-base` (16px).
- [ ] KHÔNG có em-dash trong UI text.

### 4.3 Color
- [ ] KHÔNG hardcode hex color trong component. Dùng `bg-*`, `text-*`, `border-*` Tailwind mapped từ token.
- [ ] Status: success=danger=warning phải dùng đúng token, không tự pha.
- [ ] Dark mode = default; light mode toggle persist localStorage.

### 4.4 Motion
- [ ] Hover state có transition ≤ 200ms.
- [ ] Async data có skeleton loader (không spinner xoay).
- [ ] KHÔNG có parallax, scroll-hijack, framer-motion.
- [ ] Page transition ≤ 150ms (giữa routes).

### 4.5 Component shape
- [ ] Mỗi page có 1 page title (heading 1) + 1+ section (heading 2).
- [ ] List page: table hoặc card grid, KHÔNG 3-equal feature card pattern.
- [ ] Button: `bg-accent` (primary), `bg-surface` (secondary), `bg-danger` (destructive). KHÔNG random `bg-blue-500`.
- [ ] Icon size 16/20/24 đúng ngữ cảnh.

### 4.6 Accessibility (baseline)
- [ ] Focus ring visible trên mọi interactive element.
- [ ] Label cho mọi input (không placeholder-only).
- [ ] Color contrast WCAG AA.
- [ ] Keyboard navigation cho sidebar + main.

## 5. Routes hiện có (10 app pages + 1 auth gate)

| Path | Page | Store | Ghi chú |
| --- | --- | --- | --- |
| `/unlock` | `UnlockPage.vue` | `useAuthStore` | Public route; layout ẩn sidebar/topbar. |
| `/` | `DashboardPage.vue` | composite | 4 stat tile bento (Profiles, Remotes, Active Tasks, Today's syncs). |
| `/profiles` | `ProfilesPage.vue` | `useProfilesStore` | Table: name, from → to, direction, parallel. |
| `/remotes` | `RemotesPage.vue` | `useRemotesStore` | Table + add form + test button per row. |
| `/operations` | `OperationsPage.vue` | `useOperationsStore` | Quick sync + active tasks + remote file browse. |
| `/boards` | `BoardsPage.vue` | `useBoardsStore` | Card grid (vue-flow canvas trong tương lai). |
| `/flows` | `FlowsPage.vue` | `useFlowsStore` | Card grid + add form. |
| `/schedules` | `SchedulesPage.vue` | `useSchedulesStore` | Table + enable/disable inline. |
| `/history` | `HistoryPage.vue` | `useHistoryStore` | Stats cards + by-profile + history table. |
| `/service` | `ServicePage.vue` | `useServiceStore` | Install dialog + status card + Start/Stop/Restart. |
| `/settings` | `SettingsPage.vue` | `useThemeStore` | Theme + change password + lock + self-update. |

Mỗi page dùng `useApi()` composable + typed DTO từ `src/api/types.ts`.

## 6. Thêm một surface mới (checklist)

1. Tạo route trong `src/app/router.ts`.
2. Tạo Pinia store `src/stores/<name>.ts` dùng `useApi()`.
3. Thêm DTO vào `src/api/types.ts` nếu cần.
4. Tạo page `src/pages/<Name>Page.vue`.
5. Thêm sidebar entry trong `src/components/layout/Sidebar.vue`.
6. Chạy Pre-Flight Check (§4) trên page mới.
7. Verify với `pnpm run type-check` + `pnpm run lint`.

## 7. Pre-flight quick command (CI)

```bash
cd frontend
pnpm run type-check
pnpm run lint
pnpm run build   # cũng verify Vite config + tailwind config OK
```

Nếu 3 lệnh pass + checklist §4 đối chiếu xong, PR ready for review.

## 8. Khi nào cập nhật SPEC này

- Khi dial thay đổi (DESIGN_VARIANCE / MOTION_INTENSITY / VISUAL_DENSITY).
- Khi thêm surface mới cần ngoài 4-tile bento hoặc 2-pane layout.
- Khi phát hiện convention mới cần binding (ví dụ: data table chuẩn).

Cập nhật file này + thông báo team. Không silent change.
