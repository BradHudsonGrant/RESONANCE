<script lang="ts">
  import type { LucideIcon } from '@lucide/svelte';

  /** One outline set, drawn with a 1.5 px stroke at every size. */
  let { icon: Glyph, size = 16 }: { icon: LucideIcon; size?: number } = $props();
</script>

<Glyph {size} strokeWidth={1.5} nonScalingStroke aria-hidden="true" />
