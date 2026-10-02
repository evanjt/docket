import type { Board } from './flow';

/** The project on screen and its board, for what sits outside the project's page: the palette. */
export const here = $state<{ slug: string | null; board: Board | undefined }>({ slug: null, board: undefined });

export interface Offer {
  form: string;
  label: string;
}

/** The forms the open item's panel offers, and the one the palette asked it to open. */
export const panel = $state<{ id: string | null; offers: Offer[]; asked: string | null }>({
  id: null,
  offers: [],
  asked: null,
});
