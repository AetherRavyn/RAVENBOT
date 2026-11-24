/**
 * Messages between the UI and the markdown worker.
 *
 * Kept in their own module so the worker entry and the client agree on the
 * shape without either importing the other's runtime code.
 */
import type { Token } from "marked";
import type { Token as HighlightToken } from "../highlight";

/** Lexer tokens and highlighter tokens share the transport, not the shape. */
export type WorkerToken = Token | HighlightToken;

export interface LexRequest {
  id: number;
  kind: "lex";
  body: string;
}

export interface HighlightRequest {
  id: number;
  kind: "highlight";
  code: string;
  language: string;
}

export type MarkdownWorkerRequest = LexRequest | HighlightRequest;

export interface MarkdownWorkerSuccess {
  id: number;
  ok: true;
  tokens: WorkerToken[];
}

export interface MarkdownWorkerFailure {
  id: number;
  ok: false;
  error: string;
}

export type MarkdownWorkerResponse = MarkdownWorkerSuccess | MarkdownWorkerFailure;
