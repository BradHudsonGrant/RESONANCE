<script lang="ts" module>
  export type IconName =
    | 'search'
    | 'library'
    | 'prep'
    | 'sets'
    | 'mix'
    | 'replay'
    | 'collections'
    | 'activity'
    | 'settings'
    | 'audio'
    | 'play'
    | 'pause'
    | 'star'
    | 'check'
    | 'unanalysed'
    | 'recent'
    | 'sliders'
    | 'plus'
    | 'close'
    | 'more'
    | 'info'
    | 'chevron-up'
    | 'chevron-down'
    | 'chevron-right'
    | 'headphones'
    | 'sidebar-collapse'
    | 'arrow-right';
</script>

<script lang="ts">
  import type { LucideIcon } from '@lucide/svelte';
  import Activity from '@lucide/svelte/icons/activity';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import AudioLines from '@lucide/svelte/icons/audio-lines';
  import ChartLine from '@lucide/svelte/icons/chart-line';
  import Check from '@lucide/svelte/icons/check';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Circle from '@lucide/svelte/icons/circle';
  import Clock from '@lucide/svelte/icons/clock';
  import Disc3 from '@lucide/svelte/icons/disc-3';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Headphones from '@lucide/svelte/icons/headphones';
  import Inbox from '@lucide/svelte/icons/inbox';
  import Info from '@lucide/svelte/icons/info';
  import Library from '@lucide/svelte/icons/library';
  import ListMusic from '@lucide/svelte/icons/list-music';
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close';
  import Pause from '@lucide/svelte/icons/pause';
  import Play from '@lucide/svelte/icons/play';
  import Plus from '@lucide/svelte/icons/plus';
  import Search from '@lucide/svelte/icons/search';
  import Settings from '@lucide/svelte/icons/settings';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import Star from '@lucide/svelte/icons/star';
  import X from '@lucide/svelte/icons/x';

  interface Props {
    name?: IconName;
    icon?: LucideIcon;
    size?: 'xs' | 'sm' | 'md' | number;
    filled?: boolean;
  }

  let { name, icon, size = 'sm', filled = false }: Props = $props();

  const ICONS: Record<IconName, LucideIcon> = {
    search: Search,
    library: Library,
    prep: Inbox,
    sets: ListMusic,
    mix: Disc3,
    replay: ChartLine,
    collections: FolderOpen,
    activity: Activity,
    settings: Settings,
    audio: AudioLines,
    play: Play,
    pause: Pause,
    star: Star,
    check: Check,
    unanalysed: Circle,
    recent: Clock,
    sliders: SlidersHorizontal,
    plus: Plus,
    close: X,
    more: Ellipsis,
    info: Info,
    'chevron-up': ChevronUp,
    'chevron-down': ChevronDown,
    'chevron-right': ChevronRight,
    headphones: Headphones,
    'sidebar-collapse': PanelLeftClose,
    'arrow-right': ArrowRight,
  };

  const pixelSize = $derived.by(() => {
    if (typeof size === 'number') return size;
    if (size === 'xs') return 13;
    if (size === 'md') return 18;
    return 16;
  });

  const Glyph = $derived(icon ?? (name ? ICONS[name] : Search));
</script>

<Glyph
  size={pixelSize}
  strokeWidth={1.5}
  fill={filled ? 'currentColor' : 'none'}
  nonScalingStroke
  aria-hidden="true"
/>
