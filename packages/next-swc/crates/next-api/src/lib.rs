#![feature(future_join)]
#![feature(min_specialization)]

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectOptions {
    root_path: String,
    project_path: String,
}

enum RouteType {
    Page {
        html_endpoint: EndpointVc,
        data_endpoint: EndpointVc,
    },
    PageApi {
        endpoint: EndpointVc,
    },
    AppPage {
        html_endpoint: EndpointVc,
        rsc_endpoint: EndpointVc,
    },
    AppRoute {
        endpoint: EndpointVc,
    },
}

struct Route {
    /// Including the leading slash
    pathname: String,
    ty: RouteType,
}

impl EndpointVc {
    fn write_to_disk(self) -> WrittenEndpointVc {}
}

struct WrittenEndpoint {
    /// Relative to the root_path
    server_entry_path: String,
    /// Relative to the root_path
    server_paths: Vec<String>,
    /// Relative to the root_path
    client_paths: Vec<String>,
}

#[turbo_tasks::value(transparent)]
struct Routes(Vec<RouteVc>);

impl ProjectVc {
    fn new(options: Value<ProjectOptions>) -> Self {}

    fn entry_points(self) -> RoutesVc {}

    fn hmr_events(self, identifier: String, sender: Sender) {}
}

/*
interface Project {
    constructor(options);
  // Subscription which emits the list of all current entry points.
  // A new list will be emitted whenever an entry point is added/deleted.
  entryPointsSubscribe(): AsyncIterator<EntryPoint[]>;

  // Subscription which emits an event whenever this opaque id's data changes.
  // The opaque id is received by the server from the client as part of our
  // internal Runtime subscription code.
  // A new event is emitted whenever a file change happens that effects any
  // chunk of this chunk group.
  hmrSubscribe(opaqueId): AsyncIterator<HmrEvent>;
}

interface EntryPoint {
  // The route path that this entry point represents
  // eg (?): foo/[param]/baz/page.js
  route: string;

  // The type of entry point this instance represents
  type: EntryPointType;

  // Writes this entry point and any dependencies to disk
  write(): Promise<WrittenEntryPoint>;

    // A promise which resolves when any server file changes,
  // eg getStaticProps change
    serverChangeSubscribe(): Promise<void>

  // The static page-level  config export (or collection of invidual exports)
  // Eg, `export config = { runtime: … }` or `export const runtime = …`
  // TODO: is this needed in the dev server, or just during evaluation?
  // metadata(): Record<string, string>;
}

interface WrittenEntryPoint {
  // The path where the entry point file exists
  entryPath: string;

  // The filepaths of all files that were written to disk
  paths: string[];
}

enum EntryPointType {
  AppPage,
  AppRoute,
  AppMetadata,
  Page,
  PageApi,
  // TODO: any more?
}

interface HmrEvent {
  // This is a JSON-serializable object with instructions for client's
  // Turbopack runtime.
  /// This should be sent down the websocket.
  data: any;
}
 */
