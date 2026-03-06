# @borg/react-statereducer

Typed event reducers for React features.

## Core API

- `createEventReducer(handlers)`
- `withEffects(state, effects)`
- `reduceEvents(initialState, reducer, events)`
- `useStateReducer({ initialState, reducer, runEffect })`

## Example

```ts
import {
  createEventReducer,
  useStateReducer,
  withEffects,
} from "@borg/react-statereducer";

type State = { count: number };
type Event = { type: "inc" } | { type: "dec" };
type Effect = { type: "log"; message: string };

const reducer = createEventReducer<State, Event, Effect>({
  inc: (state) =>
    withEffects(
      { ...state, count: state.count + 1 },
      [{ type: "log", message: "incremented" }]
    ),
  dec: (state) => ({ state: { ...state, count: state.count - 1 } }),
});

function Counter() {
  const { state, dispatch } = useStateReducer({
    initialState: { count: 0 },
    reducer,
    runEffect: ({ effect }) => {
      if (effect.type === "log") {
        console.log(effect.message);
      }
    },
  });

  return (
    <button type="button" onClick={() => dispatch({ type: "inc" })}>
      {state.count}
    </button>
  );
}
```
