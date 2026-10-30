/**
 * What the header search field shows when the address's query moves: the address's text, unless
 * it equals what is typed once trimmed, in which case what is typed stays, so a space typed
 * between two words survives the search firing.
 */
export function fieldText(typed: string, urlQuery: string): string {
  return typed.trim() === urlQuery ? typed : urlQuery;
}

/**
 * The page a search returns to on Escape: the one it was typed on, kept while the results are on
 * screen, and the project's overview when the search began on the results themselves.
 */
export function searchOrigin(kept: string | null, tab: string, address: string): string | null {
  return tab === 'work' ? kept : address;
}

/** Where Escape takes a search: back to the page it was typed on, else the project's overview. */
export function escapeTo(kept: string | null, overview: string): string {
  return kept ?? overview;
}
