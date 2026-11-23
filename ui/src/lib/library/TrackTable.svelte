<script lang="ts">
  import { flushSync, untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';

  import Icon from '../design/Icon.svelte';
  import TableCell from '../design/TableCell.svelte';
  import TableRow from '../design/TableRow.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { FilterField } from '../ipc/bindings/FilterField';
  import type { FilterTerm } from '../ipc/bindings/FilterTerm';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { TableColumn } from '../ipc/bindings/TableColumn';
  import type { TableLayoutChange } from '../ipc/bindings/TableLayoutChange';
  import CollectionMenu from './CollectionMenu.svelte';
  import ColumnMenu, { type MenuAnchor } from './ColumnMenu.svelte';
  import {
    cellFilters,
    cellLook,
    cellText,
    columnWidth,
    fitColumns,
    gridTemplate,
    tableWidth,
  } from './columns';
  import FilterMenu, { type FilterMenuAnchor, type FilterMenuItem } from './FilterMenu.svelte';
  import { RowHeights } from './row-heights';
  import { TableLayoutState } from './table-layout.svelte';
  import type { TrackPages } from './track-pages.svelte';

  interface Props {
    pages: TrackPages;
    /** The place in library order of the selected track. */
    selected?: number | null;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    collections?: LibraryCollection[];
    activeCollection?: LibraryCollection | null;
    /** Enter or a double click on a row. */
    onopen?: () => void;
    onpreview?: (track: LibraryTrack) => void;
    /** A double click: play the row, never pause it. Falls back to `onpreview`. */
    onplay?: (track: LibraryTrack) => void;
    /** The user moved to a row with a plain click or a key, for auto-preview. */
    onfocustrack?: (track: LibraryTrack) => void;
    onrelink?: (track: LibraryTrack) => void;
    ontogglefavourite?: (track: LibraryTrack) => void;
    /** The columns, widths, wrap and sort, kept by the core. */
    layout?: TableLayoutState;
    onaddtocollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onremovefromcollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    oncreatecollectionandadd?: (name: string, trackIds: number[]) => Promise<void> | void;
    /** The columns filters can use, for the header and cell menus. */
    filterFields?: FilterField[];
    /** A filter picked from a cell's menu. */
    onfilter?: (term: FilterTerm) => void;
    /** "Filter by" in a header's menu, for the column's id. */
    onfiltercolumn?: (column: string) => void;
    /** How many tracks are ticked, for the selection actions that Library shows above. */
    checkedCount?: number;
    allChecked?: boolean;
    /** Slices initial row mounting across short tasks during cold start. */
    progressiveStartup?: boolean;
  }

  let {
    pages,
    selected = $bindable(null),
    previewTrackId = null,
    previewPlaying = false,
    collections = [],
    activeCollection = null,
    onopen,
    onpreview,
    onplay,
    onfocustrack,
    onrelink,
    ontogglefavourite,
    layout = new TableLayoutState(),
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
    filterFields = [],
    onfilter,
    onfiltercolumn,
    checkedCount = $bindable(0),
    allChecked = $bindable(false),
    progressiveStartup = false,
  }: Props = $props();

  // Rows drawn beyond the edges of the view, so a fast scroll does not open a gap.
  const OVERSCAN = 8;
  const INITIAL_ROWS = 8;
  const STARTUP_ROW_SLICE = 4;
  const STARTUP_OVERSCAN_STEP = 2;

  const id = $props.id();
  const checkedIds = new SvelteSet<number>();
  const checkedFallback = new SvelteMap<number, LibraryTrack>();
  let gridEl: HTMLDivElement | undefined = $state();
  let viewport: HTMLDivElement | undefined = $state();
  let head: HTMLDivElement | undefined = $state();
  let rowHeight = $state(33);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let headHeight = $state(0);
  let overscan = $state(0);
  let rowCap = $state(untrack(() => (progressiveStartup ? 0 : Infinity)));

  let menuOpen = $state(false);
  let menuAnchor = $state<HTMLElement | null>(null);
  let menuTracks = $state<LibraryTrack[]>([]);
  let menuShowFavourite = $state(false);
  let menuLabel = $state('Track actions');

  const allLoadedChecked = $derived(pages.allLoadedChecked(checkedIds));
  const someChecked = $derived(checkedIds.size > 0);
  const hasAnySelection = $derived(someChecked || selected !== null);
  const resolvedMenuTracks = $derived(
    menuTracks.map((track) => pages.trackById(track.id) ?? track),
  );

  $effect(() => {
    checkedCount = checkedIds.size;
    allChecked = allLoadedChecked;
  });

  let viewportWidth = $state(0);
  let rowsEl: HTMLDivElement | undefined = $state();

  // A column being dragged wider or narrower, drawn at once and saved when the pointer lifts.
  let liveWidths = $state<Record<string, number>>({});
  const fit = $derived(fitColumns(layout.current, viewportWidth, liveWidths));
  const shownColumns = $derived(fit.shown);
  const wrap = $derived(layout.current.wrap);
  const columnsTemplate = $derived(gridTemplate(shownColumns, false, liveWidths));
  const compactTemplate = $derived(gridTemplate(shownColumns, true, liveWidths));
  const minWidth = $derived(`${tableWidth(shownColumns, liveWidths, true)}px`);

  // Wrapped rows are as tall as their text, so their tops come from measured heights. Unwrapped
  // rows are all one height and need none of this.
  const heights = new RowHeights();
  let heightsVersion = $state(0);
  $effect(() => {
    const count = wrap ? pages.total : 0;
    void pages.scrollResetCount;
    void columnsTemplate;
    const base = rowHeight;
    untrack(() => {
      heights.reset(count, base);
      heightsVersion += 1;
    });
  });

  function rowTop(index: number): number {
    return wrap ? heights.offsetOf(index) : index * rowHeight;
  }

  // The header sticks to the top of the scrolling area, so the rows show below it.
  const rowsHeight = $derived(Math.max(0, viewportHeight - headHeight));
  const first = $derived.by(() => {
    void heightsVersion;
    const top = wrap ? heights.indexAt(scrollTop) : Math.floor(scrollTop / rowHeight);
    return Math.max(0, top - overscan);
  });
  const rowsTop = $derived.by(() => {
    void heightsVersion;
    return rowTop(first);
  });
  const bodyHeight = $derived.by(() => {
    void heightsVersion;
    return wrap ? heights.total : pages.total * rowHeight;
  });
  const visibleRowsWanted = $derived(
    Math.max(
      0,
      Math.min(
        pages.total - first,
        viewportHeight > 0 ? Math.ceil(rowsHeight / rowHeight) : INITIAL_ROWS,
      ),
    ),
  );
  const unclippedShown = $derived(
    Math.max(
      0,
      Math.min(
        pages.total - first,
        viewportHeight > 0 ? Math.ceil(rowsHeight / rowHeight) + 2 * overscan : INITIAL_ROWS,
      ),
    ),
  );
  const shown = $derived(Math.min(unclippedShown, rowCap));
  const rowsInView = $derived(Math.max(1, Math.floor(rowsHeight / rowHeight)));

  let filledOnce = false;
  let startupRampDone = !untrack(() => progressiveStartup);
  let observedViewportHeight = 0;
  let observedViewportWidth = 0;
  let observedHeadHeight = 0;
  let rowHeightRead = false;

  function applyMeasuredSizes(): boolean {
    if (!viewport || !head) return false;
    const nextVh = observedViewportHeight || viewport.clientHeight;
    const nextVw = observedViewportWidth || viewport.clientWidth;
    const nextHh = observedHeadHeight || head.offsetHeight;
    let changed = false;
    if (nextVh > 0 && (nextVh !== viewportHeight || nextVw !== viewportWidth)) {
      viewportHeight = nextVh;
      viewportWidth = nextVw;
      changed = true;
    }
    if (nextHh > 0 && nextHh !== headHeight) {
      headHeight = nextHh;
      changed = true;
    }
    if (!rowHeightRead && nextVh > 0) {
      rowHeightRead = true;
      const height = parseFloat(getComputedStyle(viewport).getPropertyValue('--row-height'));
      if (height > 0 && height !== rowHeight) {
        rowHeight = height;
        changed = true;
      }
    }
    return changed;
  }

  // Reading sizes inside ResizeObserver waits until Blink has laid out the table, so mounting it
  // forces no synchronous style or layout pass. While the startup ramp is slicing rows across
  // tasks, ResizeObserver records the dimensions without mutating reactive state inside
  // BeginMainFrame's NotifyResizeObservers pass.
  $effect(() => {
    if (!viewport || !head) return;
    let resizeTimer = 0;
    const sized = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.target === viewport) {
          observedViewportHeight = entry.contentRect.height;
          observedViewportWidth = entry.contentRect.width;
        } else if (entry.target === head) {
          observedHeadHeight = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
        }
      }
      if (!startupRampDone) return;
      if (resizeTimer) return;
      resizeTimer = window.setTimeout(() => {
        resizeTimer = 0;
        applyMeasuredSizes();
      }, 0);
    });
    sized.observe(viewport);
    sized.observe(head);
    return () => {
      clearTimeout(resizeTimer);
      sized.disconnect();
    };
  });

  $effect(() => pages.want(first, first + unclippedShown));

  // During cold start, rows mount and lay out in small slices of 4 rows per task so no single
  // task or frame pays to build and shape the whole table at once.
  $effect(() => {
    if (!progressiveStartup || !viewport || !head || startupRampDone) return;
    let timer = 0;
    const step = () => {
      if (!viewport || !head) return;
      if (viewportHeight === 0) {
        let measured = false;
        flushSync(() => {
          measured = applyMeasuredSizes();
        });
        if (measured) {
          void viewport.offsetHeight;
          timer = window.setTimeout(step, 0);
          return;
        }
      }
      if (rowCap < visibleRowsWanted) {
        const nextCap = Math.min(visibleRowsWanted, rowCap + STARTUP_ROW_SLICE);
        flushSync(() => {
          rowCap = nextCap;
        });
        void viewport.offsetHeight;
        if (
          !filledOnce &&
          viewportHeight > 0 &&
          rowCap >= visibleRowsWanted &&
          pages.track(first)
        ) {
          filledOnce = true;
          performance.mark('tracks-filled');
        }
        timer = window.setTimeout(step, 0);
        return;
      }
      if (!filledOnce && viewportHeight > 0 && visibleRowsWanted > 0 && pages.track(first)) {
        filledOnce = true;
        performance.mark('tracks-filled');
      }
      if (overscan < OVERSCAN) {
        const nextOverscan = Math.min(OVERSCAN, overscan + STARTUP_OVERSCAN_STEP);
        flushSync(() => {
          rowCap = Infinity;
          overscan = nextOverscan;
        });
        void viewport.offsetHeight;
        if (nextOverscan < OVERSCAN) {
          timer = window.setTimeout(step, 0);
          return;
        }
      } else if (rowCap !== Infinity) {
        flushSync(() => {
          rowCap = Infinity;
        });
      }
      startupRampDone = true;
    };
    timer = window.setTimeout(step, 0);
    return () => clearTimeout(timer);
  });

  // Fallback for non-progressive mounts (such as unit tests or re-entering Library space).
  let overscanTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (
      progressiveStartup ||
      filledOnce ||
      viewportHeight === 0 ||
      shown === 0 ||
      !pages.track(first)
    )
      return;
    filledOnce = true;
    performance.mark('tracks-filled');
    overscanTimer = setTimeout(() => {
      overscan = OVERSCAN;
    });
  });
  $effect(() => () => clearTimeout(overscanTimer));

  $effect(() => {
    if (selected !== null) reveal(selected);
  });

  let lastScrollReset = untrack(() => pages.scrollResetCount);
  $effect(() => {
    const resetCount = pages.scrollResetCount;
    if (resetCount !== lastScrollReset) {
      lastScrollReset = resetCount;
      scrollTop = 0;
      if (viewport) viewport.scrollTop = 0;
    }
  });

  let lastCollectionId = untrack(() => pages.query.collection_id ?? null);
  $effect(() => {
    const nextCollection = pages.query.collection_id ?? null;
    if (nextCollection !== lastCollectionId) {
      lastCollectionId = nextCollection;
      clearChecked();
      menuOpen = false;
    }
  });

  // Rows drawn with text are measured after each draw. A row above the view that grows pushes
  // the scroll position down by as much, so the rows in view stay where they are.
  $effect(() => {
    if (!wrap || !rowsEl) return;
    const start = first;
    const loaded: boolean[] = [];
    for (let offset = 0; offset < shown; offset += 1)
      loaded.push(Boolean(pages.track(start + offset)));
    void columnsTemplate;
    untrack(() => {
      if (!rowsEl) return;
      const inView = heights.indexAt(viewport?.scrollTop ?? 0);
      let above = 0;
      let changed = false;
      loaded.forEach((isLoaded, offset) => {
        const row = rowsEl?.children[offset];
        if (!isLoaded || !(row instanceof HTMLElement)) return;
        const change = heights.set(start + offset, row.offsetHeight);
        if (change === 0) return;
        changed = true;
        if (start + offset < inView) above += change;
      });
      if (!changed) return;
      heightsVersion += 1;
      if (above !== 0 && viewport) {
        viewport.scrollTop += above;
        scrollTop = viewport.scrollTop;
      }
    });
  });

  function sortOf(id: string): { order: number; direction: 'asc' | 'desc' } | null {
    const at = layout.current.sort.findIndex((key) => key.column === id);
    return at < 0 ? null : { order: at, direction: layout.current.sort[at]!.direction };
  }

  function headerSort(id: string): 'ascending' | 'descending' | 'none' {
    const sort = sortOf(id);
    if (!sort || sort.order > 0) return 'none';
    return sort.direction === 'desc' ? 'descending' : 'ascending';
  }

  async function changeLayout(change: TableLayoutChange) {
    try {
      await layout.change(change);
    } catch {
      // The core keeps the layout it had, and so does the table.
    }
  }

  // A drag that moved a header ends in a click, which must not also sort it.
  let suppressClick = false;

  function sortBy(event: MouseEvent, id: string) {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    void changeLayout({ kind: 'sort', column: id, add: event.shiftKey });
  }

  function moveBy(id: string, delta: number) {
    const order = layout.current.columns.map((column) => column.id);
    const from = order.indexOf(id);
    const to = Math.max(0, Math.min(order.length - 1, from + delta));
    if (from >= 0 && to !== from) void changeLayout({ kind: 'move', column: id, to });
  }

  function onHeaderKey(event: KeyboardEvent, column: TableColumn) {
    if (event.altKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
      event.preventDefault();
      event.stopPropagation();
      moveBy(column.id, event.key === 'ArrowLeft' ? -1 : 1);
    }
  }

  let drag = $state<{ id: string; line: number } | null>(null);

  function headerCells(): HTMLElement[] {
    return Array.from(head?.querySelectorAll<HTMLElement>('[data-column]') ?? []);
  }

  function startDrag(event: PointerEvent, column: TableColumn) {
    if (event.button !== 0 || !head) return;
    if (event.target instanceof HTMLElement && event.target.closest('.resize')) return;
    const startX = event.clientX;
    let target = -1;
    let moving = false;
    const onMove = (move: PointerEvent) => {
      if (!moving && Math.abs(move.clientX - startX) < 4) return;
      moving = true;
      const cells = headerCells();
      const headLeft = head?.getBoundingClientRect().left ?? 0;
      target = cells.findIndex((cell) => {
        const rect = cell.getBoundingClientRect();
        return move.clientX < rect.left + rect.width / 2;
      });
      if (target < 0) target = cells.length;
      const edge =
        target < cells.length
          ? cells[target]!.parentElement!.getBoundingClientRect().left
          : (cells.at(-1)?.parentElement?.getBoundingClientRect().right ?? 0);
      drag = { id: column.id, line: edge - headLeft };
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      drag = null;
      if (!moving) return;
      suppressClick = true;
      setTimeout(() => (suppressClick = false));
      const shownIds = shownColumns.map((shownColumn) => shownColumn.id);
      const rest = layout.current.columns
        .map((shownColumn) => shownColumn.id)
        .filter((id) => id !== column.id);
      const before = shownIds[target];
      const lastOther = shownIds.filter((id) => id !== column.id).at(-1);
      const to =
        before !== undefined && before !== column.id
          ? rest.indexOf(before)
          : before === column.id
            ? -1
            : lastOther === undefined
              ? -1
              : rest.indexOf(lastOther) + 1;
      if (to >= 0) void changeLayout({ kind: 'move', column: column.id, to });
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  function renderedWidth(column: TableColumn): number {
    const cell = head?.querySelector<HTMLElement>(`[data-column="${CSS.escape(column.id)}"]`);
    return cell?.parentElement?.getBoundingClientRect().width ?? columnWidth(column);
  }

  function clampWidth(width: number): number {
    const { min_width_px: min, max_width_px: max } = layout.current;
    return Math.round(Math.max(min, Math.min(max, width)));
  }

  async function saveWidth(column: TableColumn, width: number) {
    await changeLayout({ kind: 'resize', column: column.id, width_px: clampWidth(width) });
    delete liveWidths[column.id];
  }

  function startResize(event: PointerEvent, column: TableColumn) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const startWidth = renderedWidth(column);
    const onMove = (move: PointerEvent) => {
      liveWidths[column.id] = clampWidth(startWidth + move.clientX - startX);
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      const width = liveWidths[column.id];
      if (width !== undefined) void saveWidth(column, width);
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  /** The width of the widest value in view, header included. */
  function fitWidth(column: TableColumn) {
    const at = shownColumns.findIndex((shownColumn) => shownColumn.id === column.id);
    if (at < 0 || !rowsEl) return;
    const cells = rowsEl.querySelectorAll<HTMLElement>(
      `:scope > [role='row'] > :nth-child(${at + 4})`,
    );
    const headerCell = head?.querySelector<HTMLElement>(`[data-column="${CSS.escape(column.id)}"]`);
    let widest = headerCell?.parentElement?.scrollWidth ?? 0;
    for (const cell of cells) widest = Math.max(widest, cell.scrollWidth);
    void saveWidth(column, widest + 2);
  }

  function onResizeKey(event: KeyboardEvent, column: TableColumn) {
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      event.stopPropagation();
      const step = (event.shiftKey ? 1 : 8) * (event.key === 'ArrowLeft' ? -1 : 1);
      void saveWidth(column, renderedWidth(column) + step);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      event.stopPropagation();
      fitWidth(column);
    }
  }

  let columnMenuOpen = $state(false);
  let columnMenuAnchor = $state<MenuAnchor | null>(null);
  let columnMenuReturn: HTMLElement | null = null;
  // The header that was right-clicked, which the menu offers to filter by.
  let columnMenuFilter = $state<FilterField | null>(null);

  let cellMenuOpen = $state(false);
  let cellMenuAnchor = $state<FilterMenuAnchor | null>(null);
  let cellMenuItems = $state<FilterMenuItem[]>([]);

  function openCellMenu(event: MouseEvent) {
    const target = event.target instanceof HTMLElement ? event.target : null;
    const cell = target?.closest<HTMLElement>('[data-column]');
    const row = target?.closest<HTMLElement>('[aria-rowindex]');
    if (!cell || !row || !onfilter) return;
    const index = Number(row.getAttribute('aria-rowindex')) - 2;
    const track = pages.track(index);
    const field = filterFields.find((item) => item.column === cell.dataset.column);
    if (!track || !field) return;
    const offers = cellFilters(field, track);
    if (offers.length === 0) return;
    event.preventDefault();
    cellMenuItems = offers.map((offer) => ({
      label: offer.label,
      run: () => onfilter(offer.term),
    }));
    cellMenuAnchor = {
      left: event.clientX,
      right: event.clientX,
      top: event.clientY,
      bottom: event.clientY,
    };
    cellMenuOpen = true;
  }

  function openColumnMenu(event: MouseEvent) {
    const button = event.currentTarget as HTMLElement;
    if (columnMenuOpen) {
      closeColumnMenu();
      return;
    }
    columnMenuReturn = button;
    columnMenuFilter = null;
    columnMenuAnchor = button.getBoundingClientRect();
    columnMenuOpen = true;
  }

  function openColumnMenuAt(event: MouseEvent) {
    event.preventDefault();
    const target = event.target instanceof HTMLElement ? event.target : null;
    const headerColumn = target?.closest<HTMLElement>('[data-column]')?.dataset.column;
    columnMenuFilter = onfiltercolumn
      ? (filterFields.find((item) => item.column === headerColumn) ?? null)
      : null;
    columnMenuReturn =
      target
        ?.closest<HTMLElement>('[role="columnheader"]')
        ?.querySelector<HTMLElement>('button, [tabindex="0"]') ??
      gridEl ??
      null;
    columnMenuAnchor = {
      left: event.clientX,
      right: event.clientX,
      top: event.clientY,
      bottom: event.clientY,
    };
    columnMenuOpen = true;
  }

  function closeColumnMenu() {
    columnMenuOpen = false;
    if (columnMenuReturn?.isConnected) columnMenuReturn.focus();
    else gridEl?.focus();
  }

  function reveal(index: number) {
    if (!viewport) return;
    const top = rowTop(index);
    const height = wrap ? heights.heightOf(index) : rowHeight;
    const visible = viewport.clientHeight - headHeight;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + height > viewport.scrollTop + visible)
      viewport.scrollTop = top + height - visible;
  }

  function select(index: number) {
    if (pages.total > 0) selected = Math.min(Math.max(index, 0), pages.total - 1);
  }

  const playRow = $derived(onplay ?? onpreview);

  /** A move the user made to a row, which auto-preview follows. */
  function moveTo(index: number) {
    select(index);
    const track = selected === null ? undefined : pages.track(selected);
    if (track) onfocustrack?.(track);
  }

  function addChecked(track: LibraryTrack) {
    checkedIds.add(track.id);
    checkedFallback.set(track.id, track);
  }

  function removeChecked(trackId: number) {
    checkedIds.delete(trackId);
    checkedFallback.delete(trackId);
  }

  function toggleChecked(track: LibraryTrack) {
    if (checkedIds.has(track.id)) {
      removeChecked(track.id);
    } else {
      addChecked(track);
    }
  }

  function clearChecked() {
    checkedIds.clear();
    checkedFallback.clear();
  }

  function deselectAll() {
    clearChecked();
    selected = null;
  }

  // Library shows the selection actions in place of its filter pills, so the table never moves.
  export function selectAllLoaded() {
    for (const track of pages.loadedTracks()) {
      addChecked(track);
    }
  }

  export function clearSelection() {
    deselectAll();
    gridEl?.focus();
  }

  export function addSelectionToCollection(anchor: HTMLElement) {
    openMenuForTracks(resolveCheckedTracks(), anchor, {
      showFavourite: false,
      label: 'Add selected tracks to collection',
    });
  }

  export function checkedTrackIds(): number[] {
    return Array.from(checkedIds);
  }

  export function removeSelectionFromCollection() {
    if (activeCollection) {
      void removeTracksFromCollection(activeCollection.id, Array.from(checkedIds));
    }
  }

  function selectRange(fromIndex: number, toIndex: number) {
    if (pages.total <= 0) return;
    const clampedFrom = Math.min(Math.max(fromIndex, 0), pages.total - 1);
    const clampedTo = Math.min(Math.max(toIndex, 0), pages.total - 1);
    for (
      let idx = Math.min(clampedFrom, clampedTo);
      idx <= Math.max(clampedFrom, clampedTo);
      idx += 1
    ) {
      const item = pages.track(idx);
      if (item) addChecked(item);
    }
  }

  function resolveCheckedTracks(): LibraryTrack[] {
    const out: LibraryTrack[] = [];
    for (const trackId of checkedIds) {
      const live = pages.trackById(trackId) ?? checkedFallback.get(trackId);
      if (live) out.push(live);
    }
    return out;
  }

  function toggleHeaderChecked() {
    if (allLoadedChecked) {
      deselectAll();
      return;
    }
    selectAllLoaded();
  }

  function onRowClick(event: MouseEvent, index: number, track: LibraryTrack | undefined) {
    if (!track) {
      select(index);
      return;
    }
    if (event.shiftKey) {
      selectRange(selected ?? index, index);
      select(index);
      return;
    }
    if (event.ctrlKey || event.metaKey) {
      if (!someChecked) {
        if (selected === index) {
          selected = null;
        } else {
          if (selected !== null) {
            const current = pages.track(selected);
            if (current) addChecked(current);
          }
          addChecked(track);
          select(index);
        }
      } else if (checkedIds.has(track.id)) {
        removeChecked(track.id);
        if (selected === index) selected = null;
      } else {
        addChecked(track);
        select(index);
      }
      return;
    }
    if (event.detail <= 1 && selected === index && !someChecked) {
      selected = null;
      return;
    }
    if (event.detail > 1) select(index);
    else moveTo(index);
  }

  function onTickClick(event: MouseEvent, index: number, track: LibraryTrack | undefined) {
    event.stopPropagation();
    if (!track) return;
    if (event.shiftKey) {
      selectRange(selected ?? index, index);
      select(index);
      return;
    }
    const isChecked = checkedIds.has(track.id);
    const isSelected = selected === index;
    const visuallyTicked = someChecked ? isChecked : isSelected;
    if (visuallyTicked) {
      removeChecked(track.id);
      if (isSelected) selected = null;
      return;
    }
    if (!someChecked && selected !== null) {
      const current = pages.track(selected);
      if (current) addChecked(current);
    }
    addChecked(track);
  }

  function onViewportClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    if (!target) return;
    if (target.closest('.head, [role="row"]')) return;
    if (someChecked || selected !== null) {
      deselectAll();
    }
  }

  function openMenuForTracks(
    tracks: LibraryTrack[],
    anchor: HTMLElement | null,
    options: { showFavourite: boolean; label: string },
  ) {
    if (tracks.length === 0) return;
    menuTracks = tracks;
    menuAnchor = anchor;
    menuShowFavourite = options.showFavourite;
    menuLabel = options.label;
    menuOpen = true;
  }

  function closeMenu() {
    const anchor = menuAnchor;
    menuOpen = false;
    if (anchor?.isConnected && anchor !== gridEl && anchor.tabIndex >= 0) {
      anchor.focus();
    } else {
      gridEl?.focus();
    }
  }

  async function removeTracksFromCollection(collectionId: number, trackIds: number[]) {
    await onremovefromcollection?.(collectionId, trackIds);
    if (activeCollection?.id === collectionId) {
      for (const trackId of trackIds) {
        checkedIds.delete(trackId);
        checkedFallback.delete(trackId);
      }
    }
  }

  function targetTracksForAction(fallbackTrack?: LibraryTrack): LibraryTrack[] {
    if (fallbackTrack) {
      if (checkedIds.size > 1 && checkedIds.has(fallbackTrack.id)) {
        return resolveCheckedTracks();
      }
      return [fallbackTrack];
    }
    if (checkedIds.size > 0) return resolveCheckedTracks();
    if (selected !== null) {
      const track = pages.track(selected);
      if (track) return [track];
    }
    return [];
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (menuOpen || columnMenuOpen) return;
    if (event.target instanceof HTMLElement && event.target.closest('.head')) return;
    const at = selected ?? -1;
    const moves: Record<string, number> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      PageDown: at + rowsInView,
      PageUp: at - rowsInView,
      Home: 0,
      End: pages.total - 1,
    };
    const to = moves[event.key];
    if (to !== undefined) {
      event.preventDefault();
      if (event.shiftKey && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
        selectRange(selected ?? 0, to);
      }
      moveTo(to);
    } else if (event.key === 'Enter' && selected !== null) {
      event.preventDefault();
      onopen?.();
    } else if (event.key.toLowerCase() === 'x' && selected !== null) {
      const track = pages.track(selected);
      if (track) {
        event.preventDefault();
        toggleChecked(track);
      }
    } else if (event.key.toLowerCase() === 'f' && selected !== null) {
      const track = pages.track(selected);
      if (track && ontogglefavourite) {
        event.preventDefault();
        ontogglefavourite(track);
      }
    } else if (event.key.toLowerCase() === 'c') {
      const targets = targetTracksForAction();
      if (targets.length > 0) {
        event.preventDefault();
        const anchor =
          (selected !== null ? document.getElementById(`${id}-row-${selected}`) : null) ??
          gridEl ??
          null;
        openMenuForTracks(targets, anchor, {
          showFavourite: false,
          label: 'Add to collection',
        });
      }
    } else if ((event.key === 'Delete' || event.key === 'Backspace') && activeCollection != null) {
      const targets = targetTracksForAction();
      if (targets.length > 0) {
        event.preventDefault();
        void removeTracksFromCollection(
          activeCollection.id,
          targets.map((track) => track.id),
        );
      }
    } else if (event.key === 'Escape' && (checkedIds.size > 0 || selected !== null)) {
      event.preventDefault();
      deselectAll();
    }
  }

  function typing(target: EventTarget | null): boolean {
    return (
      target instanceof HTMLElement &&
      (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
    );
  }

  function onWindowKeyDown(event: KeyboardEvent) {
    if (
      event.defaultPrevented ||
      menuOpen ||
      event.altKey ||
      document.querySelector('dialog[open], [role="menu"]')
    ) {
      return;
    }
    if (typing(event.target)) return;
    const command = event.ctrlKey || event.metaKey;
    if (command && event.key.toLowerCase() === 'a') {
      if (pages.total > 0) {
        event.preventDefault();
        if (event.shiftKey) {
          deselectAll();
        } else {
          selectAllLoaded();
          gridEl?.focus();
        }
      }
      return;
    }
    if (!command && event.key === 'Escape' && (checkedIds.size > 0 || selected !== null)) {
      event.preventDefault();
      deselectAll();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeyDown} />

{#snippet sortButton(id: string, label: string, numeric: boolean, column?: TableColumn)}
  {@const sort = sortOf(id)}
  <button
    type="button"
    class={['sort-btn', { numeric, sorted: sort !== null }]}
    onclick={(event) => sortBy(event, id)}
    onkeydown={(event) => {
      // Enter sorts here, because the click it would make does not carry Shift.
      if (event.key === 'Enter' && !event.altKey && !event.ctrlKey && !event.metaKey) {
        event.preventDefault();
        void changeLayout({ kind: 'sort', column: id, add: event.shiftKey });
      } else if (column) {
        onHeaderKey(event, column);
      }
    }}
  >
    {#if sort}
      <span class={['sort-arrow', { secondary: sort.order > 0 }]}>
        <Icon
          name={sort.direction === 'desc' ? 'chevron-down' : 'chevron-up'}
          size={sort.order > 0 ? 9 : 11}
        />
        {#if sort.order > 0}
          <span class="sort-order" aria-hidden="true">2</span>
        {/if}
      </span>
    {/if}
    <span class="sort-label">{label}</span>
    {#if sort && sort.order > 0}<span class="visually-hidden">, second sort</span>{/if}
  </button>
{/snippet}

<div
  bind:this={gridEl}
  class="table"
  role="grid"
  tabindex="0"
  aria-label="Tracks"
  aria-rowcount={pages.total + 1}
  aria-activedescendant={selected !== null && selected >= first && selected < first + shown
    ? `${id}-row-${selected}`
    : undefined}
  class:wrap
  style:--columns={columnsTemplate}
  style:--columns-compact={compactTemplate}
  style:--table-width={minWidth}
  {onkeydown}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="viewport"
    bind:this={viewport}
    onclick={onViewportClick}
    onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
  >
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="head" role="rowgroup" bind:this={head} oncontextmenu={openColumnMenuAt}>
      <TableRow header aria-rowindex={1} style="grid-template-columns: var(--active-columns)">
        <TableCell header compact>
          <button
            type="button"
            class={['tick', 'header-tick', { filled: hasAnySelection }]}
            role="checkbox"
            aria-label={allLoadedChecked ? 'Deselect all tracks' : 'Select all tracks'}
            aria-checked={allLoadedChecked ? 'true' : hasAnySelection ? 'mixed' : 'false'}
            tabindex={-1}
            onclick={toggleHeaderChecked}
          >
            {#if allLoadedChecked}
              <Icon name="check" size="xs" />
            {:else if hasAnySelection}
              <span class="tick-dash" aria-hidden="true"></span>
            {/if}
          </button>
        </TableCell>
        <TableCell header compact><span class="visually-hidden">Status</span></TableCell>
        <TableCell header numeric ariaSort={headerSort('index')}>
          {@render sortButton('index', '#', true)}
        </TableCell>
        {#each shownColumns as column (column.id)}
          <TableCell
            header
            numeric={column.numeric}
            compact={column.id === 'artwork'}
            ariaSort={column.sortable ? headerSort(column.id) : undefined}
          >
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class={['head-cell', { dragging: drag?.id === column.id }]}
              data-column={column.id}
              onpointerdown={(event) => startDrag(event, column)}
            >
              {#if column.sortable}
                {@render sortButton(column.id, column.label, column.numeric, column)}
              {:else}
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <span
                  class="head-label"
                  tabindex="0"
                  onkeydown={(event) => onHeaderKey(event, column)}
                >
                  {#if column.id === 'artwork'}
                    <span class="visually-hidden">{column.label}</span>
                  {:else}
                    {column.label}
                  {/if}
                </span>
              {/if}
              <!-- A focused separator is the resize handle, which ARIA counts as a widget. -->
              <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
              <span
                class="resize"
                role="separator"
                aria-orientation="vertical"
                aria-label="Width of {column.label}"
                aria-valuemin={layout.current.min_width_px}
                aria-valuemax={layout.current.max_width_px}
                aria-valuenow={Math.round(columnWidth(column, liveWidths[column.id]))}
                tabindex="0"
                onpointerdown={(event) => startResize(event, column)}
                ondblclick={() => fitWidth(column)}
                onkeydown={(event) => onResizeKey(event, column)}
              ></span>
            </div>
          </TableCell>
        {/each}
        <TableCell header compact>
          <div class="head-actions">
            {#if fit.hidden.length > 0}
              <button
                type="button"
                class="head-button hidden-count"
                aria-label="{fit.hidden.length} columns hidden, too narrow"
                aria-haspopup="menu"
                onclick={openColumnMenu}>+{fit.hidden.length}</button
              >
            {/if}
            <button
              type="button"
              class="head-button"
              aria-label="Choose columns"
              aria-haspopup="menu"
              aria-expanded={columnMenuOpen}
              onclick={openColumnMenu}
            >
              <Icon name="columns" size="xs" />
            </button>
          </div>
        </TableCell>
      </TableRow>
      {#if drag}
        <span class="drop-line" style:left="{drag.line}px" aria-hidden="true"></span>
      {/if}
    </div>
    <div class="body" role="rowgroup" style:height="{bodyHeight}px">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="rows"
        bind:this={rowsEl}
        style:transform="translateY({rowsTop}px)"
        oncontextmenu={openCellMenu}
      >
        {#each { length: shown }, offset}
          {@const index = first + offset}
          {@const track = pages.track(index)}
          {@const isSelected = index === selected}
          {@const isChecked = track ? checkedIds.has(track.id) : false}
          {@const isVisuallyTicked = someChecked ? isChecked : isSelected}
          {@const isMissing = Boolean(track && track.missing_since != null)}
          {@const isPreviewTrack = Boolean(track && previewTrackId === track.id)}
          {@const isPreviewPlaying = isPreviewTrack && previewPlaying && !isMissing}
          <TableRow
            id="{id}-row-{index}"
            aria-rowindex={index + 2}
            selected={isSelected}
            checked={isChecked && !isSelected}
            style="grid-template-columns: var(--active-columns)"
            onclick={(event) => onRowClick(event, index, track)}
            ondblclick={() => {
              select(index);
              if (track && isMissing && onrelink) onrelink(track);
              else if (track && playRow) playRow(track);
              else onopen?.();
            }}
          >
            <!-- The cells stay while a row waits for its page, so a jump changes text and does
                 not rebuild them. -->
            <TableCell compact>
              <button
                type="button"
                class={['tick', { filled: isVisuallyTicked }]}
                role="checkbox"
                aria-label={track ? `Tick ${track.title}` : 'Tick row'}
                aria-checked={isChecked}
                tabindex={-1}
                disabled={!track}
                onclick={(event) => onTickClick(event, index, track)}
                ondblclick={(event) => event.stopPropagation()}
              >
                {#if isVisuallyTicked}
                  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
                    <path
                      d="M2.2 6.2 4.8 8.8 9.8 3.4"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
                {/if}
              </button>
            </TableCell>
            <TableCell compact>
              <button
                type="button"
                class={[
                  'status-cell',
                  {
                    shown: Boolean(track) && (isMissing || isSelected || isPreviewTrack),
                    playing: isPreviewPlaying,
                    missing: isMissing,
                  },
                ]}
                aria-label={track
                  ? isMissing
                    ? `Missing file for ${track.title} — Relink`
                    : isPreviewPlaying
                      ? `Pause ${track.title}`
                      : `Play ${track.title}`
                  : 'Play track'}
                aria-pressed={isMissing ? undefined : isPreviewPlaying}
                disabled={!track}
                tabindex={-1}
                onclick={(event) => {
                  event.stopPropagation();
                  if (track) {
                    select(index);
                    if (isMissing) onrelink?.(track);
                    else onpreview?.(track);
                  }
                }}
                ondblclick={(event) => event.stopPropagation()}
              >
                <span class="status-icon" aria-hidden="true">
                  <Icon
                    name={isMissing ? 'missing' : isPreviewPlaying ? 'audio' : 'play'}
                    size={13}
                    filled={!isMissing && !isPreviewPlaying}
                  />
                </span>
              </button>
            </TableCell>
            <TableCell numeric size="sm" tone="muted">
              {track ? index + 1 : ''}
            </TableCell>
            {#each shownColumns as column (column.id)}
              {#if column.id === 'artwork'}
                <TableCell compact>
                  <span
                    class="thumb"
                    data-column={column.id}
                    style:background-image={track?.artwork
                      ? `url("${artworkUrl(track.artwork)}")`
                      : 'none'}
                  ></span>
                </TableCell>
              {:else}
                {@const look = cellLook(column)}
                {@const value = track ? cellText(column, track) : ''}
                <TableCell
                  numeric={look.numeric}
                  mono={look.mono}
                  tone={look.tone}
                  weight={look.weight}
                >
                  <span
                    class="value"
                    data-column={column.id}
                    title={!wrap && value ? value : undefined}>{value}</span
                  >
                </TableCell>
              {/if}
            {/each}

            <TableCell compact>
              <div class="row-actions">
                <button
                  type="button"
                  class={['row-star', { fav: Boolean(track?.favourite) }]}
                  aria-label={track ? `Favourite ${track.title}` : 'Favourite track'}
                  aria-pressed={Boolean(track?.favourite)}
                  disabled={!track}
                  tabindex={-1}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (track) ontogglefavourite?.(track);
                  }}
                  ondblclick={(event) => event.stopPropagation()}
                >
                  <Icon name="star" size="xs" filled={Boolean(track?.favourite)} />
                </button>
                <button
                  type="button"
                  class="more"
                  aria-label={track ? `Actions for ${track.title}` : 'Track actions'}
                  aria-haspopup="menu"
                  disabled={!track}
                  tabindex={-1}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (track) {
                      const targets = targetTracksForAction(track);
                      openMenuForTracks(targets, event.currentTarget, {
                        showFavourite: targets.length === 1,
                        label: `Actions for ${track.title}`,
                      });
                    }
                  }}
                  ondblclick={(event) => event.stopPropagation()}
                >
                  <Icon name="more" size="xs" />
                </button>
              </div>
            </TableCell>
            {#if !track}
              <span class="pending" aria-label="Loading"></span>
            {/if}
          </TableRow>
        {/each}
      </div>
    </div>
  </div>
</div>

{#if columnMenuOpen}
  <ColumnMenu
    open={columnMenuOpen}
    anchor={columnMenuAnchor}
    layout={layout.current}
    hidden={fit.hidden.map((column) => column.id)}
    filterColumn={columnMenuFilter}
    onfilter={(column) => {
      columnMenuOpen = false;
      onfiltercolumn?.(column);
    }}
    onchange={(change) => void changeLayout(change)}
    onclose={closeColumnMenu}
  />
{/if}

{#if cellMenuOpen}
  <FilterMenu
    open={cellMenuOpen}
    anchor={cellMenuAnchor}
    label="Filter by this cell"
    items={cellMenuItems}
    returnFocus={gridEl ?? null}
    onclose={() => (cellMenuOpen = false)}
  />
{/if}

{#if menuOpen && resolvedMenuTracks.length > 0}
  <CollectionMenu
    open={menuOpen && resolvedMenuTracks.length > 0}
    anchorEl={menuAnchor}
    label={menuLabel}
    {collections}
    selectedTracks={resolvedMenuTracks}
    {activeCollection}
    showFavourite={menuShowFavourite}
    onToggleFavourite={ontogglefavourite}
    onRelink={onrelink}
    onAdd={onaddtocollection}
    onRemove={removeTracksFromCollection}
    onCreateAndAdd={oncreatecollectionandadd}
    onClose={closeMenu}
  />
{/if}

<style>
  .table {
    --active-columns: var(--columns);
    container: track-table / inline-size;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    padding: 0 var(--space-5);
  }

  .table:focus-visible {
    outline: none;
  }

  .table:focus-visible:not(:has([aria-selected='true'])) .viewport {
    border-radius: var(--radius-sm);
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .table:focus-visible :global(.row.selected) {
    outline-width: var(--focus-ring-width);
    outline-offset: var(--focus-ring-offset);
  }

  .viewport {
    height: 100%;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .head,
  .body {
    min-width: var(--table-width);
  }

  .head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--color-background);
  }

  /* What changes as the table scrolls is laid out and painted inside here alone. */
  .body {
    overflow: hidden;
    contain: strict;
  }

  .rows {
    will-change: transform;
  }

  .rows > :global([role='row']) {
    contain: layout paint;
  }

  .tick {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    padding: 0;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-background);
    cursor: pointer;
  }

  .tick.filled {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .header-tick {
    border-color: var(--color-border-control);
    cursor: pointer;
  }

  .tick-dash {
    width: var(--space-2);
    height: var(--selection-bar-width);
    background: var(--color-background);
  }

  .tick:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .status-cell {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-xs);
    height: var(--control-height-xs);
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .status-cell:disabled {
    cursor: default;
  }

  .status-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text);
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .status-cell.shown .status-icon,
  :global(.row:hover) .status-cell:not(:disabled) .status-icon,
  .status-cell:focus-visible:not(:disabled) .status-icon {
    opacity: 1;
  }

  .status-cell.missing .status-icon,
  :global(.row:hover) .status-cell:not(.shown, :disabled) .status-icon {
    color: var(--color-text-2);
  }

  .status-cell:hover:not(:disabled),
  .status-cell:focus-visible:not(:disabled) {
    background: var(--color-surface-2);
  }

  .status-cell:hover:not(:disabled) .status-icon {
    color: var(--color-text);
  }

  .sort-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    width: calc(100% + var(--space-4));
    height: var(--table-header-height);
    margin: 0 calc(var(--space-2) * -1);
    padding: 0 var(--space-2);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .sort-btn:not(.numeric) {
    flex-direction: row-reverse;
    justify-content: flex-end;
  }

  .sort-btn.numeric {
    justify-content: flex-end;
  }

  .sort-btn:hover,
  .sort-btn.sorted {
    color: var(--color-text);
  }

  .sort-arrow {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
  }

  .sort-arrow.secondary {
    color: var(--color-text-2);
  }

  .sort-order {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .sort-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .head-cell {
    position: relative;
    display: flex;
    align-items: center;
    height: var(--table-header-height);
    touch-action: none;
  }

  .head-cell.dragging {
    margin: 0 calc(var(--space-2) * -1);
    padding: 0 var(--space-2);
    border-radius: var(--radius-xs);
    background: var(--color-raised);
    outline: var(--border-width) solid var(--color-border-control);
    outline-offset: calc(-1 * var(--border-width));
  }

  .head-label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .head-label:focus-visible {
    border-radius: var(--radius-xs);
    outline: var(--focus-ring-width) solid var(--color-border-control);
  }

  .resize {
    position: absolute;
    top: var(--space-1-5);
    right: calc(var(--space-2) * -1);
    bottom: var(--space-1-5);
    width: var(--space-1-5);
    cursor: col-resize;
  }

  .resize::after {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: var(--border-width);
    background: transparent;
    content: '';
  }

  .resize:hover::after,
  .resize:focus-visible::after,
  .resize:active::after {
    background: var(--color-border-control);
  }

  .resize:focus-visible {
    outline: none;
  }

  .drop-line {
    position: absolute;
    top: var(--space-1);
    bottom: var(--space-1);
    width: var(--selection-bar-width);
    margin-left: calc(var(--selection-bar-width) / -2);
    background: var(--color-text);
    pointer-events: none;
  }

  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-0-5);
  }

  .head-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: var(--control-height-keybadge);
    height: var(--control-height-keybadge);
    padding: 0 var(--space-0-5);
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .head-button:hover,
  .head-button:focus-visible,
  .head-button[aria-expanded='true'] {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .thumb {
    width: var(--control-height-keybadge);
    height: var(--control-height-keybadge);
    border-radius: var(--radius-xs);
    background-color: var(--color-surface-2);
    background-position: center;
    background-size: cover;
  }

  /* Rows grow to their longest value, up to three lines, and anything longer is cut there. */
  .wrap .rows > :global([role='row']) {
    height: auto;
    min-height: var(--row-height);
    padding-block: var(--space-1-5);
  }

  .wrap .rows :global([role='cell']:not(.compact)) {
    display: -webkit-box;
    overflow-wrap: anywhere;
    white-space: normal;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-0-5);
  }

  .row-star,
  .more {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-keybadge);
    height: var(--control-height-keybadge);
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      opacity var(--duration-fast) var(--ease-out);
  }

  .row-star {
    opacity: 0;
  }

  .row-star.fav {
    color: var(--color-text);
    opacity: 1;
  }

  :global(.row:hover) .row-star:not(:disabled),
  .row-star:focus-visible:not(:disabled) {
    opacity: 1;
  }

  .row-star:hover:not(:disabled),
  .row-star:focus-visible:not(:disabled),
  .more:hover:not(:disabled),
  .more:focus-visible:not(:disabled) {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .row-star:disabled {
    cursor: default;
    opacity: 0;
  }

  .more:disabled {
    cursor: default;
    opacity: 0.4;
  }

  @container track-table (width < 600px) {
    .viewport {
      --active-columns: var(--columns-compact);
    }
  }

  @media (forced-colors: active) {
    .drop-line {
      background: CanvasText;
    }

    .resize:hover::after,
    .resize:focus-visible::after,
    .resize:active::after {
      background: Highlight;
    }

    .head-button[aria-expanded='true'],
    .head-cell.dragging {
      outline: var(--border-width) solid Highlight;
    }

    .thumb {
      border: var(--border-width) solid CanvasText;
    }

    .tick.filled {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }

    .status-cell.playing {
      border: var(--border-width) solid Highlight;
    }

    .status-cell.missing {
      border: var(--border-width) solid CanvasText;
    }
  }

  /* A row's layout containment makes it the box this is placed in. */
  .pending {
    position: absolute;
    top: calc(50% - var(--space-1-5) / 2);
    right: var(--space-2);
    left: var(--space-2);
    height: var(--space-1-5);
    border-radius: var(--radius-xs);
    background: var(--color-surface-2);
  }
</style>
