import type { IconKind } from "../state/settings";

/** Draw a cell marker centered on (cx, cy); `size` is the icon's bounding box. */
export function drawIcon(ctx: CanvasRenderingContext2D, kind: IconKind, cx: number, cy: number, size: number, color: string): void {
  if (kind === "none") return;
  ctx.save();
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const s = size;
  switch (kind) {
    case "circle":
      ctx.beginPath();
      ctx.arc(cx, cy, s * 0.42, 0, Math.PI * 2);
      ctx.fill();
      break;
    case "dot":
      ctx.beginPath();
      ctx.arc(cx, cy, Math.max(1.2, s * 0.13), 0, Math.PI * 2);
      ctx.fill();
      break;
    case "square":
      ctx.fillRect(cx - s * 0.36, cy - s * 0.36, s * 0.72, s * 0.72);
      break;
    case "diamond":
      ctx.beginPath();
      ctx.moveTo(cx, cy - s * 0.5);
      ctx.lineTo(cx + s * 0.5, cy);
      ctx.lineTo(cx, cy + s * 0.5);
      ctx.lineTo(cx - s * 0.5, cy);
      ctx.closePath();
      ctx.fill();
      break;
    case "check":
      ctx.lineWidth = Math.max(1.5, s * 0.16);
      ctx.beginPath();
      ctx.moveTo(cx - s * 0.36, cy + s * 0.02);
      ctx.lineTo(cx - s * 0.1, cy + s * 0.3);
      ctx.lineTo(cx + s * 0.4, cy - s * 0.3);
      ctx.stroke();
      break;
    case "x":
      ctx.lineWidth = Math.max(1.5, s * 0.15);
      ctx.beginPath();
      ctx.moveTo(cx - s * 0.32, cy - s * 0.32);
      ctx.lineTo(cx + s * 0.32, cy + s * 0.32);
      ctx.moveTo(cx + s * 0.32, cy - s * 0.32);
      ctx.lineTo(cx - s * 0.32, cy + s * 0.32);
      ctx.stroke();
      break;
    case "dash":
      ctx.lineWidth = Math.max(1.5, s * 0.16);
      ctx.beginPath();
      ctx.moveTo(cx - s * 0.36, cy);
      ctx.lineTo(cx + s * 0.36, cy);
      ctx.stroke();
      break;
  }
  ctx.restore();
}
