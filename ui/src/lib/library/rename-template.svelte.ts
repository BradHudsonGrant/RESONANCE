import type { RenamePreview } from '../ipc/bindings/RenamePreview';
import type { RenameReport } from '../ipc/bindings/RenameReport';
import type { RenameScope } from '../ipc/bindings/RenameScope';

export const defaultTemplateText = '{artist} - {title} ({mix})';

export const templateTokens = [
  'artist',
  'title',
  'mix',
  'bpm',
  'key',
  'label',
  'catno',
  'year',
  'track',
] as const;

const storageKey = 'resonance-filename-template';

function stored(): string {
  try {
    return localStorage.getItem(storageKey) || defaultTemplateText;
  } catch {
    return defaultTemplateText;
  }
}

/** The template the Rename sheet starts from. Settings changes it. */
export const filenameTemplate = $state({ current: stored() });

export function setFilenameTemplate(text: string) {
  filenameTemplate.current = text;
  try {
    localStorage.setItem(storageKey, text);
  } catch {
    // The template still applies for this session.
  }
}

export interface RenamePreviewClient {
  preview: (scope: RenameScope, template: string, limit: number) => Promise<RenamePreview>;
}

/**
 * Previews a template as it is typed. A request goes out at once when none is in flight; while
 * one is, only the latest template waits, so the preview never falls behind the field.
 */
export class LivePreview {
  preview = $state<RenamePreview | null>(null);
  error = $state<string | null>(null);
  /** The template `preview` is for, or `null` when the last one did not parse. */
  template = $state<string | null>(null);
  #client: () => RenamePreviewClient;
  #limit: number;
  #inFlight = false;
  #next: { scope: RenameScope; template: string } | null = null;

  /** `client` is read on every request, so a component can pass its current prop. */
  constructor(client: () => RenamePreviewClient, limit: number) {
    this.#client = client;
    this.#limit = limit;
  }

  request(scope: RenameScope, template: string) {
    this.#next = { scope, template };
    if (!this.#inFlight) void this.#run();
  }

  async #run() {
    this.#inFlight = true;
    try {
      while (this.#next) {
        const { scope, template } = this.#next;
        this.#next = null;
        try {
          const preview = await this.#client().preview(scope, template, this.#limit);
          if (this.#next) continue;
          this.preview = preview;
          this.template = template;
          this.error = null;
        } catch (failure) {
          if (this.#next) continue;
          this.template = null;
          this.error = failure instanceof Error ? failure.message : String(failure);
        }
      }
    } finally {
      this.#inFlight = false;
    }
  }
}

export function renameSummary(report: RenameReport): string {
  const files = report.renamed === 1 ? '1 file' : `${report.renamed.toLocaleString('en-GB')} files`;
  const failed = report.failed.length;
  if (failed === 0) return `Renamed ${files}.`;
  return `Renamed ${files}. ${failed.toLocaleString('en-GB')} could not be renamed.`;
}
