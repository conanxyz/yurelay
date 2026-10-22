import type { Task } from './types';
/** Bounded list metadata; history_detail owns the full document payload. */
export type HistorySummary = Pick<Task, 'id' | 'revision' | 'source' | 'status' | 'original_text' | 'created_at'> & {
  source_kinds?: string[];
};
