/**
 * Row heights for a table whose rows can wrap. Every row starts at the base height and changes
 * once it has been drawn and measured. A Fenwick tree over the heights finds a row's top, and
 * the row at a scroll position, in a few steps however long the library is.
 */
export class RowHeights {
  #base: number;
  #count: number;
  #heights: Float64Array;
  #tree: Float64Array;

  constructor(count = 0, base = 33) {
    this.#base = base;
    this.#count = count;
    this.#heights = new Float64Array(0);
    this.#tree = new Float64Array(0);
    this.reset(count, base);
  }

  get count(): number {
    return this.#count;
  }

  get total(): number {
    return this.offsetOf(this.#count);
  }

  /** Forgets every measured row. */
  reset(count: number, base = this.#base) {
    this.#base = base;
    this.#count = count;
    this.#heights = new Float64Array(count).fill(base);
    this.#tree = new Float64Array(count + 1);
    for (let at = 1; at <= count; at += 1) {
      this.#tree[at]! += base;
      const parent = at + (at & -at);
      if (parent <= count) this.#tree[parent]! += this.#tree[at]!;
    }
  }

  heightOf(index: number): number {
    return this.#heights[index] ?? this.#base;
  }

  /** Records a measured height and returns how much it changed. */
  set(index: number, height: number): number {
    if (index < 0 || index >= this.#count) return 0;
    const change = height - this.#heights[index]!;
    if (Math.abs(change) < 0.5) return 0;
    this.#heights[index] = height;
    for (let at = index + 1; at <= this.#count; at += at & -at) this.#tree[at]! += change;
    return change;
  }

  /** The top of row `index`, which for `count` is the height of the whole table. */
  offsetOf(index: number): number {
    let sum = 0;
    for (let at = Math.min(index, this.#count); at > 0; at -= at & -at) sum += this.#tree[at]!;
    return sum;
  }

  /** The row under `offset` pixels from the top. */
  indexAt(offset: number): number {
    if (offset <= 0 || this.#count === 0) return 0;
    let at = 0;
    let rest = offset;
    let step = 1;
    while (step * 2 <= this.#count) step *= 2;
    for (; step > 0; step = Math.floor(step / 2)) {
      const next = at + step;
      if (next <= this.#count && this.#tree[next]! <= rest) {
        at = next;
        rest -= this.#tree[next]!;
      }
    }
    return Math.min(at, this.#count - 1);
  }
}
