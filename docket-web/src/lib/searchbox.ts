/**
 * What the header search field shows when the address's query moves: the address's text, unless
 * it equals what is typed once trimmed, in which case what is typed stays, so a space typed
 * between two words survives the search firing.
 */
export function fieldText(typed: string, urlQuery: string): string {
  return typed.trim() === urlQuery ? typed : urlQuery;
}
