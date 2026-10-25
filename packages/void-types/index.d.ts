// Types for the modules built into Void Browser: `void` and `void/jsx-runtime`.

declare module "void" {
  export interface Signal<T> {
    (): T;
    set(value: T): void;
    update(fn: (value: T) => T): void;
    peek(): T;
  }
  export type Accessor<T> = () => T;

  export function signal<T>(initial: T): Signal<T>;
  export function signal<T>(): Signal<T | undefined>;
  export function computed<T>(fn: () => T): Accessor<T>;
  export function effect(fn: () => void): () => void;
  export function batch<T>(fn: () => T): T;
  export function untrack<T>(fn: () => T): T;
  export function root<T>(fn: (dispose: () => void) => T): T;
  export function onCleanup(fn: () => void): void;
  export function now(): number;

  export function render(app: () => JSX.Element): void;

  export function For<T>(props: { each: Accessor<T[]>; children: (item: T, index: number) => JSX.Element }): JSX.Element;
  export function Show(props: { when: Accessor<unknown>; fallback?: JSX.Element; children?: JSX.Element }): JSX.Element;

  export interface VoidEvent {
    type: string;
    key: string;
    value: string;
    target: { id: number; value: string };
    stopPropagation(): void;
  }

  type MaybeAccessor<T> = T | Accessor<T>;

  export interface HTMLProps {
    class?: MaybeAccessor<string | Record<string, unknown> | undefined>;
    style?: MaybeAccessor<string | Record<string, string | number>>;
    id?: string;
    placeholder?: string;
    value?: MaybeAccessor<string>;
    ref?: (node: JSX.Element) => void;
    children?: unknown;
    onClick?: (e: VoidEvent) => void;
    onInput?: (e: VoidEvent) => void;
    onKeyDown?: (e: VoidEvent) => void;
    [attr: string]: unknown;
  }

  global {
    namespace JSX {
      type Element = unknown;
      interface IntrinsicElements {
        [tag: string]: HTMLProps;
      }
      interface ElementChildrenAttribute {
        children: {};
      }
    }
  }
}

declare module "void/jsx-runtime" {
  export function jsx(type: unknown, props: unknown): JSX.Element;
  export const jsxs: typeof jsx;
  export function Fragment(props: { children?: unknown }): JSX.Element;
}
