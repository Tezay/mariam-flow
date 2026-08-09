/** Receiving a density model and choosing which one is in service. */

import { rename, send, type WriteOutcome } from './http';
import type { Status } from './status';

/** What a model says about itself. */
export type ModelManifest = { name: string; trained_at: string; sessions: number };

/** One model the appliance holds. */
export type StoredModel = {
  id: string;
  manifest?: ModelManifest;
  imported_at_us: number;
  window_us: number;
  receivers: number;
  active: boolean;
  has_evaluation: boolean;
};

/** What one recorded capture contributed to a training run. */
export type TrainingSession = {
  session_id: string;
  windows: number;
  /** Windows per class, in `empty, low, medium, saturated` order. */
  support: number[];
};

/** How a model scored against captures it was never shown. */
export type Evaluation = {
  accuracy: number;
  baseline_accuracy: number;
  /** Rows are truth, columns are prediction, in `empty..saturated` order. */
  confusion: number[][];
  windows: number;
  splits: number;
  receivers: string[];
  sessions: TrainingSession[];
};

/** A model and what its training run said about it. */
export type ModelDetail = StoredModel & { evaluation?: Evaluation };

/**
 * Takes a trained model bundle into service.
 *
 * Sent as raw bytes rather than a form: the appliance reads one archive, and
 * a multipart envelope would add a parser for nothing.
 */
export async function importModel(file: File): Promise<WriteOutcome> {
  try {
    const response = await fetch('/api/model', {
      method: 'POST',
      headers: { 'content-type': 'application/gzip' },
      body: file,
    });
    if (response.ok) {
      return { kind: 'ok', status: (await response.json()) as Status };
    }
    const failure = (await response.json().catch(() => null)) as { error?: string } | null;
    return { kind: 'refused', message: failure?.error ?? '' };
  } catch {
    return { kind: 'error' };
  }
}

/** Lists every model the appliance holds, newest first. */
export async function fetchModels(): Promise<StoredModel[]> {
  try {
    const response = await fetch('/api/models');
    return response.ok ? ((await response.json()) as StoredModel[]) : [];
  } catch {
    return [];
  }
}

/** Reads one model with the scores its training run earned. */
export async function fetchModel(id: string): Promise<ModelDetail | null> {
  try {
    const response = await fetch(`/api/models/${encodeURIComponent(id)}`);
    return response.ok ? ((await response.json()) as ModelDetail) : null;
  } catch {
    return null;
  }
}

/** Puts one of the stored models back into service. */
export async function useModel(id: string): Promise<WriteOutcome> {
  return send('POST', `/api/models/${encodeURIComponent(id)}`, undefined);
}

/** Removes a stored model. */
export async function forgetModel(id: string): Promise<boolean> {
  try {
    const response = await fetch(`/api/models/${encodeURIComponent(id)}`, { method: 'DELETE' });
    return response.ok;
  } catch {
    return false;
  }
}

/** Gives a stored model a new name. */
export async function renameModel(id: string, name: string): Promise<boolean> {
  return rename(`/api/models/${encodeURIComponent(id)}`, name);
}
