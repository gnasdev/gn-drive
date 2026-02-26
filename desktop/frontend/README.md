# GN Drive Frontend

Angular 21 frontend for GN Drive with Tailwind CSS styling.

## Structure

```
frontend/src/app/
├── app.service.ts          # Main backend orchestration
├── tab.service.ts          # Tab state management
├── app.component.ts        # Root component
├── app.routes.ts           # Route definitions
├── home/                   # Dashboard/sync operations
│   ├── home.component.ts
│   ├── home.component.html
│   └── home.component.css
├── profiles/               # Profile management
│   ├── profiles.component.ts
│   └── profile-edit.component.ts
├── remotes/                # Remote management
│   └── remotes.component.ts
├── components/             # Shared components
│   ├── sync-status/        # Progress display
│   ├── error-display/      # Error notifications
│   └── toast/              # Toast messages
├── services/               # Additional services
│   ├── error.service.ts
│   ├── logging.service.ts
│   └── navigation.service.ts
└── models/                 # TypeScript interfaces
    ├── sync-status.interface.ts
    └── events.ts           # Event type definitions
```

## State Management

Uses RxJS BehaviorSubjects:

```typescript
// app.service.ts
readonly currentAction$ = new BehaviorSubject<Action | undefined>(undefined);
readonly configInfo$ = new BehaviorSubject<ConfigInfo>(initialConfig);
readonly syncStatus$ = new BehaviorSubject<SyncStatus | null>(null);

// Usage in components
this.appService.configInfo$.subscribe(config => {
    this.profiles = config.profiles;
});
```

## Event Handling

Frontend listens to backend events:

```typescript
// app.service.ts
Events.On("tofe", (event) => {
    const parsedEvent = parseEvent(event.data);

    if (isSyncEvent(parsedEvent)) {
        this.handleSyncEvent(parsedEvent);
    } else if (isConfigEvent(parsedEvent)) {
        this.handleConfigEvent(parsedEvent);
    }
});
```

Event types and guards in `models/events.ts`.

## Backend Bindings

TypeScript bindings generated from Go:

```typescript
import {
    Sync,
    GetConfigInfo,
    AddRemote
} from "wailsjs/desktop/backend/app";
import * as models from "wailsjs/desktop/backend/models/models";

// Usage
const taskId = await Sync("pull", profile);
const config = await GetConfigInfo();
```

Regenerate bindings:
```bash
cd desktop && wails3 generate bindings
```

## Components

### HomeComponent

Dashboard with:
- Multi-tab sync operations
- Profile selection
- Real-time progress display
- Start/stop controls

### ProfilesComponent

Profile management:
- List all profiles
- Create new profiles
- Navigate to edit

### ProfileEditComponent

Profile editor:
- Name, from/to paths
- Include/exclude patterns
- Bandwidth and parallel settings

### RemotesComponent

Remote management:
- List configured remotes
- Add new remotes (OAuth flow)
- Delete remotes

### SyncStatusComponent

Progress display:
- Percentage complete
- Transfer speed
- Files transferred
- Current file
- ETA

## Styling

Uses Tailwind CSS with custom classes in `styles.scss`:

```scss
// Custom button styles
.btn-primary { @apply ... }

// Card styles
.card { @apply ... }

// Form inputs
.input-field { @apply ... }
```

Dark mode supported via `dark:` prefix classes.

## Development

### Start Dev Server

```bash
npm start -- --port 9245
```

### Build

```bash
npm run build
```

### Lint

```bash
npm run lint
```

### Test

```bash
npm test
```

## Adding Components

```bash
ng generate component components/my-component
```

Follow patterns:
- Use RxJS for async state
- Handle backend events properly
- Clean up subscriptions in ngOnDestroy

## Dependencies

- Angular 21
- Tailwind CSS
- RxJS
- @wailsio/runtime (Wails integration)
