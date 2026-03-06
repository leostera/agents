import * as React from "react";

export type StateEvent = {
  type: string;
};

export type ReducerResult<State, Effect = never> = {
  state: State;
  effects?: readonly Effect[];
};

export type StateReducer<State, Event extends StateEvent, Effect = never> = (
  state: State,
  event: Event
) => ReducerResult<State, Effect>;

export type EventHandlers<State, Event extends StateEvent, Effect = never> = {
  [Type in Event["type"]]: (
    state: State,
    event: Extract<Event, { type: Type }>
  ) => ReducerResult<State, Effect>;
};

type UnknownEventHandler<State, Event extends StateEvent, Effect> = (
  state: State,
  event: Event
) => ReducerResult<State, Effect>;

const NO_EFFECTS: readonly never[] = [];

export function createEventReducer<
  State,
  Event extends StateEvent,
  Effect = never,
>(
  handlers: EventHandlers<State, Event, Effect>,
  options: {
    onUnknownEvent?: UnknownEventHandler<State, Event, Effect>;
  } = {}
): StateReducer<State, Event, Effect> {
  return (state, event) => {
    const handler = handlers[event.type as Event["type"]] as
      | ((state: State, event: Event) => ReducerResult<State, Effect>)
      | undefined;
    if (handler) return handler(state, event);
    if (options.onUnknownEvent) return options.onUnknownEvent(state, event);
    return { state };
  };
}

export function withEffects<State, Effect = never>(
  state: State,
  effects: readonly Effect[] = NO_EFFECTS as readonly Effect[]
): ReducerResult<State, Effect> {
  return { state, effects };
}

export function reduceEvents<State, Event extends StateEvent, Effect = never>(
  initialState: State,
  reducer: StateReducer<State, Event, Effect>,
  events: readonly Event[]
): ReducerResult<State, Effect> {
  let state = initialState;
  const effects: Effect[] = [];
  for (const event of events) {
    const result = reducer(state, event);
    state = result.state;
    if (result.effects && result.effects.length > 0) {
      effects.push(...result.effects);
    }
  }
  return withEffects(state, effects);
}

export type EffectRuntime<State, Event extends StateEvent, Effect> = {
  dispatch: (event: Event) => void;
  getState: () => State;
  effect: Effect;
};

export type UseStateReducerOptions<
  State,
  Event extends StateEvent,
  Effect = never,
> = {
  initialState: State | (() => State);
  reducer: StateReducer<State, Event, Effect>;
  runEffect?: (runtime: EffectRuntime<State, Event, Effect>) => void;
};

export type UseStateReducerResult<State, Event extends StateEvent> = {
  state: State;
  dispatch: (event: Event) => void;
  send: (event: Event) => void;
  getState: () => State;
};

export function useStateReducer<
  State,
  Event extends StateEvent,
  Effect = never,
>({
  initialState,
  reducer,
  runEffect,
}: UseStateReducerOptions<State, Event, Effect>): UseStateReducerResult<
  State,
  Event
> {
  const [state, setState] = React.useState(initialState);
  const stateRef = React.useRef(state);
  const effectsRef = React.useRef<Effect[]>([]);

  React.useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const getState = React.useCallback(() => stateRef.current, []);

  const dispatch = React.useCallback(
    (event: Event) => {
      setState((currentState) => {
        const result = reducer(currentState, event);
        if (result.effects && result.effects.length > 0) {
          effectsRef.current.push(...result.effects);
        }
        stateRef.current = result.state;
        return result.state;
      });
    },
    [reducer]
  );

  React.useEffect(() => {
    if (!runEffect || effectsRef.current.length === 0) return;
    const effects = effectsRef.current.splice(0, effectsRef.current.length);
    for (const effect of effects) {
      runEffect({ effect, dispatch, getState });
    }
  }, [dispatch, getState, runEffect, state]);

  return {
    state,
    dispatch,
    send: dispatch,
    getState,
  };
}
