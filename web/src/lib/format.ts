// Formatting per the design (batch 1, 1c): metric units, 24-hour time,
// "7 Oct 2026" dates, a space before %, tabular numbers in the UI.

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

function scale(bytes: number): number {
	let i = 0;
	let v = Math.abs(bytes);
	while (v >= 1024 && i < UNITS.length - 1) {
		v /= 1024;
		i++;
	}
	return i;
}

function num(v: number): string {
	if (v >= 100 || Number.isInteger(v)) return Math.round(v).toString();
	return v.toFixed(1).replace(/\.0$/, '');
}

/** "412 GB", "3.6 TB". Binary multiples, as Proxmox reports them. */
export function bytes(value: number | null | undefined): string {
	if (value == null || Number.isNaN(value)) return '–';
	const i = scale(value);
	return `${num(value / 1024 ** i)} ${UNITS[i]}`;
}

/** "27.1 / 32 GB" in the unit of the total. */
export function usedOf(used: number | null | undefined, total: number | null | undefined): string {
	if (used == null || total == null) return '–';
	const i = scale(total);
	return `${num(used / 1024 ** i)} / ${num(total / 1024 ** i)} ${UNITS[i]}`;
}

/** "38.2 MB/s" from bytes per second. */
export function rate(bytesPerSecond: number | null | undefined): string {
	if (bytesPerSecond == null || Number.isNaN(bytesPerSecond)) return '–';
	const i = scale(bytesPerSecond);
	return `${num(bytesPerSecond / 1024 ** i)} ${UNITS[i]}/s`;
}

/** "12 %" from a fraction (0.12). */
export function pct(fraction: number | null | undefined, digits = 0): string {
	if (fraction == null || Number.isNaN(fraction)) return '–';
	return `${(fraction * 100).toFixed(digits)} %`;
}

/** "99.3 %" from a percentage (99.3). */
export function percent(value: number | null | undefined): string {
	if (value == null || Number.isNaN(value)) return '–';
	return `${value % 1 === 0 ? value.toFixed(0) : value.toFixed(1)} %`;
}

export function ms(value: number | null | undefined): string {
	return value == null ? '–' : `${Math.round(value)} ms`;
}

export function celsius(value: number | null | undefined): string {
	return value == null ? '–' : `${Math.round(value)} °C`;
}

/** "12 d 4 h", "3 h 12 min", "5 min", "18 s". */
export function duration(seconds: number | null | undefined): string {
	if (seconds == null || seconds < 0) return '–';
	const d = Math.floor(seconds / 86400);
	const h = Math.floor((seconds % 86400) / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	if (d > 0) return h > 0 ? `${d} d ${h} h` : `${d} d`;
	if (h > 0) return m > 0 ? `${h} h ${m} min` : `${h} h`;
	if (m > 0) return `${m} min`;
	return `${Math.round(seconds)} s`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function toDate(value: string | Date): Date {
	return typeof value === 'string' ? new Date(value) : value;
}

const pad = (n: number) => n.toString().padStart(2, '0');

/** "7 Oct 2026" */
export function date(value: string | Date | null | undefined): string {
	if (!value) return '–';
	const d = toDate(value);
	return `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** "14:02" */
export function time(value: string | Date | null | undefined): string {
	if (!value) return '–';
	const d = toDate(value);
	return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "7 Oct 2026, 14:02" */
export function dateTime(value: string | Date | null | undefined): string {
	if (!value) return '–';
	return `${date(value)}, ${time(value)}`;
}

/** "just now", "2 min ago", "3 h ago", "4 d ago". Pair with `dateTime` as a tooltip. */
export function relative(value: string | Date | null | undefined, now: Date = new Date()): string {
	if (!value) return '–';
	const s = Math.round((now.getTime() - toDate(value).getTime()) / 1000);
	if (s < 0) return 'just now';
	if (s < 10) return 'just now';
	if (s < 60) return `${s} s ago`;
	if (s < 3600) return `${Math.floor(s / 60)} min ago`;
	if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
	return `${Math.floor(s / 86400)} d ago`;
}

/** "CT 123" or "VM 200". */
export function guestId(type: string, vmid: number): string {
	return `${type === 'lxc' ? 'CT' : 'VM'} ${vmid}`;
}
