# crossref Phase 1b — Docker Layer-2 conformance + snapshot promotion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Layer-2 Docker pipeline so soap-server's controlled doc/literal
responses are validated by an independent Java XML oracle and diffed against an Apache
CXF reference server, then **promote** the matching snapshots from `unverified` to
`verified` — the first point where external correctness (not our own say-so) enters the
harness.

**Architecture:** A `docker compose` topology runs three services on one network: (1) our
**controlled soap-server** (a new `publish=false` binary serving the Echo/EchoNamed
controlled WSDL with deterministic handlers), (2) an **Apache CXF** reference server
publishing JAX-WS endpoints for the *same* controlled services with deterministic impls,
and (3) a **Java XML oracle** (JDK `com.sun.net.httpserver` exposing `POST /validate`
using JAXP/Xerces XSD validation and `POST /c14n` using Apache Santuario exclusive
C14N). A **Rust Layer-2 orchestrator** (in the existing `crossref` crate) brings the
topology up, replays the conformance scenarios against both servers, validates every
response via the oracle, masks (path-scoped, in Rust) then canonicalizes (via the
oracle) both responses, diffs them, assigns a §5.7 verdict, and on `pass` promotes the
snapshot (`unverified`→`verified`) writing the oracle-canonical bytes as the new golden.
Everything spec-sensitive (validation, C14N, reference SOAP framing) lives in containers
— the host needs only Docker + cargo.

**Tech Stack:** Rust (`crossref` crate; `bollard` or the `docker` CLI for compose;
`reqwest` for oracle/server HTTP; `quick-xml` for path-scoped masking; `serde`/`toml`),
Docker + docker compose, Java 21 (containerised: JAXP/Xerces, Apache Santuario, Apache
CXF via `JaxWsServerFactoryBean`), built with Maven inside multi-stage images.

**Spec:** `docs/superpowers/specs/2026-06-02-crossref-harness-design.md` — this plan
implements §4.2 Layer 2, §4.3 containerised authorities, §5.2 promotion, §5.4 manifest,
§5.6 schema-validation levels, §5.7 verdict model, §6 layout, §7 CI (Layer-2 workflow),
and §8 phase 1b. **Explicitly out of scope (→ a later Phase 1c plan, and reported as
still-`unverified`):** WS-Security conformance (CXF WSS4J), multi-service & SOAP 1.1
conformance framing, and ALL interop (CXF/Zeep clients driving our server).

**Conformance scenario set for this plan** (the doc/literal cases both servers can serve
identically): `op_echo_success`, `op_echo_missing_required`, `op_echo_empty_text`,
`op_echo_special_chars`, `doc_literal_named_present`, `doc_literal_named_missing`,
`ns_on_envelope`, `ns_on_header`, `ns_on_body`, `ns_on_operation`,
`ns_on_nested_payload`, `ns_prefix_shadowing`. (12 scenarios.) The other 10 stay
`unverified`.

---

## File Structure

- `crossref/src/bin/controlled_server.rs` (create) — runnable soap-server serving the
  controlled Echo/EchoNamed WSDL with the deterministic handlers (reuses the handler
  logic factored out of `sut.rs`). Listens on `0.0.0.0:8080`.
- `crossref/src/handlers.rs` (create) — the Echo/EchoNamed handler logic + `extract_*`,
  factored out of `sut.rs` so BOTH the in-process SUT (Layer 1) and the controlled
  server binary (Layer 2) share one implementation. `sut.rs` is refactored to use it.
- `crossref/manifest.toml` (create) — comparator registry (§5.4): oracle + CXF, each
  `image@sha256:…` digest-pinned, role, version, scenarios.
- `crossref/comparators/oracle/` (create) — Java XML oracle: `pom.xml`, `Oracle.java`,
  `Dockerfile` (multi-stage Maven build → JRE runtime).
- `crossref/comparators/cxf/` (create) — CXF reference server: `pom.xml`, controlled
  service impls + a `Main.java` publishing the endpoints, `Dockerfile`.
- `crossref/docker-compose.yml` (create) — Layer-2 topology (controlled-server + cxf +
  oracle).
- `crossref/src/oracle.rs` (create) — Rust client for the oracle (`validate`, `c14n`).
- `crossref/src/layer2/mod.rs` (create) — orchestrator module: compose lifecycle,
  scenario drive, verdict model, promotion, report. Split into focused submodules:
  - `crossref/src/layer2/compose.rs` — `docker compose up/down` + readiness wait.
  - `crossref/src/layer2/verdict.rs` — the §5.7 `Verdict` enum + diff→verdict logic.
  - `crossref/src/layer2/promote.rs` — snapshot promotion (write golden + flip status).
  - `crossref/src/layer2/report.rs` — per-scenario report + still-`unverified` count.
- `crossref/src/bin/layer2.rs` (create) — CLI entrypoint: `cargo run -p crossref --bin
  layer2 -- [--promote] [--keep-up]`.
- `crossref/snapshots/status.toml` (modify, by the orchestrator at runtime) — entries
  flip to `verified` on promotion.
- `.github/workflows/layer2.yml` (create) — Linux+Docker workflow on `workflow_dispatch`
  + nightly `schedule`.
- `crossref/Cargo.toml` (modify) — add `reqwest` (blocking or tokio), `bollard` optional
  (we use the `docker` CLI via `std::process` to avoid a heavy dep — see Task 6).
- `crossref/README.md` (modify) — document Layer 2 + how to run it.

---

## Task 1: Factor controlled handlers into a shared module

**Files:**
- Create: `crossref/src/handlers.rs`
- Modify: `crossref/src/sut.rs`, `crossref/src/lib.rs`

The Echo/EchoNamed handler closures + `extract_first_text_by_suffix` currently live in
`sut.rs`. The Layer-2 controlled-server binary needs the *same* handlers. Factor them out
with zero behavior change (Layer-1 tests must stay green).

- [ ] **Step 1: Create `crossref/src/handlers.rs`** with the handler logic moved verbatim
  from `sut.rs`. Public surface:

```rust
//! Deterministic controlled-service handlers, shared by the in-process Layer-1 SUT
//! (`sut.rs`) and the Layer-2 controlled-server binary (`bin/controlled_server.rs`).
//! Keeping one implementation guarantees Layer 1 and Layer 2 exercise identical handler
//! behavior.

use bytes::Bytes;
use soap_server::{FnHandler, SoapFault};

/// Extract the full decoded text of the first element whose local name ends with
/// `suffix`, reconstructing entity references and preserving significant whitespace.
pub fn extract_first_text_by_suffix(body: &[u8], suffix: &str) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(false);
    let mut in_target = false;
    let mut acc = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                if std::str::from_utf8(local.as_ref()).unwrap_or("").ends_with(suffix) {
                    in_target = true;
                    acc.clear();
                }
            }
            Ok(Event::Text(t)) if in_target => {
                acc.push_str(&t.decode().unwrap_or_default());
            }
            Ok(Event::GeneralRef(r)) if in_target => {
                let name = r.decode().unwrap_or_default();
                match name.as_ref() {
                    "lt" => acc.push('<'),
                    "gt" => acc.push('>'),
                    "amp" => acc.push('&'),
                    "apos" => acc.push('\''),
                    "quot" => acc.push('"'),
                    _ => {}
                }
            }
            Ok(Event::End(_)) if in_target => return Some(std::mem::take(&mut acc)),
            Ok(Event::Eof) | Err(_) => return if in_target { Some(acc) } else { None },
            _ => {}
        }
    }
}

pub fn extract_text(body: &[u8]) -> Option<String> {
    extract_first_text_by_suffix(body, "Text")
}
pub fn extract_value(body: &[u8]) -> Option<String> {
    extract_first_text_by_suffix(body, "Value")
}

/// `Echo` handler: echoes the request `Text` verbatim (re-escaped) in an `EchoResponse`.
pub fn echo_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let text = extract_text(&body).unwrap_or_default();
        let resp = format!(
            r#"<c:EchoResponse xmlns:c="http://crossref.example/controlled"><c:Text>{}</c:Text></c:EchoResponse>"#,
            soap_server::escape_text(&text)
        );
        Ok::<Bytes, SoapFault>(Bytes::from(resp))
    })
}

/// `EchoNamed` handler: echoes the request `Value` element (named complex type).
pub fn echo_named_handler() -> impl soap_server::SoapHandler {
    FnHandler::new(|body: Bytes| async move {
        let value = extract_value(&body).unwrap_or_default();
        let resp = format!(
            r#"<c:EchoNamedResponse xmlns:c="http://crossref.example/controlled"><c:Value>{}</c:Value></c:EchoNamedResponse>"#,
            soap_server::escape_text(&value)
        );
        Ok::<Bytes, SoapFault>(Bytes::from(resp))
    })
}
```

> NOTE: copy the EXACT current body of these functions from `sut.rs` (the response
> element names `EchoResponse`/`EchoNamedResponse` and the `Value`/`Text` wrapper must
> match what the existing snapshots were captured with). If the current `sut.rs`
> `echo_named_handler` uses a different response element name, use THAT name here.

- [ ] **Step 2: Add `pub mod handlers;` to `crossref/src/lib.rs`.**

- [ ] **Step 3: Refactor `sut.rs`** to delete its local copies and re-use
  `crate::handlers::{echo_handler, echo_named_handler, extract_text, extract_value}` in
  `controlled_base()` and any tests. Change nothing else.

- [ ] **Step 4: Verify Layer-1 unchanged.**

Run: `cargo test -p crossref`
Expected: same pass counts as before (16 unit + 2 integration). 

Run: `git status --short crossref/snapshots`
Expected: EMPTY — no snapshot changes (behavior identical).

- [ ] **Step 5: Commit**

```bash
git add crossref/src/handlers.rs crossref/src/lib.rs crossref/src/sut.rs
git commit -m "refactor(crossref): share controlled handlers between SUT and Layer-2 server"
```

---

## Task 2: Controlled soap-server binary

**Files:**
- Create: `crossref/src/bin/controlled_server.rs`
- Modify: `crossref/Cargo.toml` (ensure `tokio` is a normal dep with `rt-multi-thread`,
  `macros`; `axum` is available transitively via soap-server's `into_router()` returning
  an `axum::Router` — confirm and add `axum`/`tokio` to crossref `[dependencies]` if the
  bin needs them directly).

- [ ] **Step 1: Confirm the serve API.** Read `soap-server/src/server.rs` — `SoapService::into_router()` returns an `axum::Router`. The binary builds the controlled service and serves the router with `axum::serve`.

- [ ] **Step 2: Write the binary** `crossref/src/bin/controlled_server.rs`:

```rust
//! Layer-2 controlled soap-server: serves the controlled Echo/EchoNamed WSDL with the
//! shared deterministic handlers. Listens on 0.0.0.0:8080 inside the compose network.

use crossref::handlers::{echo_handler, echo_named_handler};
use soap_server::ServerBuilder;

const CONTROLLED_WSDL: &[u8] = include_bytes!("../../fixtures/controlled.wsdl");

#[tokio::main]
async fn main() {
    let svc = ServerBuilder::from_wsdl_bytes(CONTROLLED_WSDL.to_vec())
        .path("/soap")
        .handler("Echo", echo_handler())
        .handler("EchoNamed", echo_named_handler())
        .build()
        .expect("controlled service must build");
    let router = svc.into_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("bind 0.0.0.0:8080");
    eprintln!("controlled-server listening on 0.0.0.0:8080/soap");
    axum::serve(listener, router).await.expect("serve");
}
```

> If `axum`/`tokio`/their features are not already direct deps of `crossref`, add them to
> `crossref/Cargo.toml` `[dependencies]` matching the versions soap-server uses (read its
> `Cargo.toml`). Use `tokio = { version = "1", features = ["rt-multi-thread", "macros", "net"] }`.

- [ ] **Step 3: Verify it builds and serves locally.**

Run: `cargo build -p crossref --bin controlled_server`
Expected: PASS.

Run (manual smoke, background): `cargo run -p crossref --bin controlled_server &` then
`curl -s -X POST localhost:8080/soap -H 'content-type: application/soap+xml; charset=utf-8' --data-binary @crossref/scenarios/op_echo_success.request.xml` ; kill the server.
Expected: an `EchoResponse` containing `hello`.

- [ ] **Step 4: Commit**

```bash
git add crossref/src/bin/controlled_server.rs crossref/Cargo.toml
git commit -m "feat(crossref): controlled soap-server binary for Layer-2 topology"
```

---

## Task 3: Java XML oracle container (validate + C14N)

**Files:**
- Create: `crossref/comparators/oracle/pom.xml`
- Create: `crossref/comparators/oracle/src/main/java/crossref/oracle/Oracle.java`
- Create: `crossref/comparators/oracle/Dockerfile`

The oracle is the independent authority for all spec-sensitive grading. It exposes two
endpoints over plain HTTP (no framework — JDK `com.sun.net.httpserver`):
- `POST /c14n` — body = XML bytes; returns exclusive-C14N (omit comments) bytes via
  Apache Santuario. `200` + canonical bytes, or `400` + error text.
- `POST /validate?schema=<id>` — body = XML bytes; validates against the named schema
  (`soap11-envelope`, `soap12-envelope`, or a payload schema id registered from the
  controlled XSD). Returns JSON `{"valid":true}` or `{"valid":false,"errors":["…"]}`.

- [ ] **Step 1: `pom.xml`** pinning Santuario + bundling the controlled XSD/WSDL on the
  classpath. (Xerces ships in the JDK via JAXP; Santuario provides exclusive C14N.)

```xml
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>crossref</groupId>
  <artifactId>oracle</artifactId>
  <version>1.0.0</version>
  <packaging>jar</packaging>
  <properties>
    <maven.compiler.release>21</maven.compiler.release>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.apache.santuario</groupId>
      <artifactId>xmlsec</artifactId>
      <version>4.0.3</version>
    </dependency>
  </dependencies>
  <build>
    <finalName>oracle</finalName>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-shade-plugin</artifactId>
        <version>3.5.1</version>
        <executions>
          <execution>
            <phase>package</phase>
            <goals><goal>shade</goal></goals>
            <configuration>
              <transformers>
                <transformer implementation="org.apache.maven.plugins.shade.resource.ManifestResourceTransformer">
                  <mainClass>crossref.oracle.Oracle</mainClass>
                </transformer>
              </transformers>
            </configuration>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
```

- [ ] **Step 2: `Oracle.java`** — the HTTP service.

```java
package crossref.oracle;

import com.sun.net.httpserver.HttpServer;
import org.apache.xml.security.Init;
import org.apache.xml.security.c14n.Canonicalizer;

import javax.xml.XMLConstants;
import javax.xml.transform.stream.StreamSource;
import javax.xml.validation.Schema;
import javax.xml.validation.SchemaFactory;
import javax.xml.validation.Validator;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

public class Oracle {
    private static final Map<String, Schema> SCHEMAS = new HashMap<>();

    public static void main(String[] args) throws Exception {
        Init.init(); // Santuario
        SchemaFactory sf = SchemaFactory.newInstance(XMLConstants.W3C_XML_SCHEMA_NS_URI);
        // Schemas are bundled on the classpath under /schemas/. Each maps an id → .xsd.
        register(sf, "soap11-envelope", "/schemas/soap11-envelope.xsd");
        register(sf, "soap12-envelope", "/schemas/soap12-envelope.xsd");
        register(sf, "controlled",      "/schemas/controlled.xsd");

        HttpServer server = HttpServer.create(new InetSocketAddress("0.0.0.0", 8081), 0);
        server.createContext("/healthz", ex -> respond(ex, 200, "ok".getBytes()));
        server.createContext("/c14n", Oracle::handleC14n);
        server.createContext("/validate", Oracle::handleValidate);
        server.setExecutor(null);
        System.err.println("oracle listening on 0.0.0.0:8081");
        server.start();
    }

    private static void register(SchemaFactory sf, String id, String resource) throws Exception {
        try (InputStream in = Oracle.class.getResourceAsStream(resource)) {
            if (in == null) throw new IllegalStateException("missing schema resource: " + resource);
            SCHEMAS.put(id, sf.newSchema(new StreamSource(in)));
        }
    }

    private static void handleC14n(com.sun.net.httpserver.HttpExchange ex) {
        try {
            byte[] body = ex.getRequestBody().readAllBytes();
            Canonicalizer c = Canonicalizer.getInstance(
                Canonicalizer.ALGO_ID_C14N_EXCL_OMIT_COMMENTS);
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            c.canonicalize(body, out, false);
            respond(ex, 200, out.toByteArray());
        } catch (Exception e) {
            respond(ex, 400, ("c14n error: " + e.getMessage()).getBytes(StandardCharsets.UTF_8));
        }
    }

    private static void handleValidate(com.sun.net.httpserver.HttpExchange ex) {
        try {
            String q = ex.getRequestURI().getQuery(); // schema=<id>
            String id = q != null && q.startsWith("schema=") ? q.substring(7) : "";
            Schema schema = SCHEMAS.get(id);
            byte[] body = ex.getRequestBody().readAllBytes();
            if (schema == null) {
                respond(ex, 200, ("{\"valid\":false,\"errors\":[\"unknown schema id: " + id + "\"]}")
                    .getBytes(StandardCharsets.UTF_8));
                return;
            }
            Validator v = schema.newValidator();
            final StringBuilder errs = new StringBuilder();
            v.setErrorHandler(new org.xml.sax.ErrorHandler() {
                public void warning(org.xml.sax.SAXParseException e) {}
                public void error(org.xml.sax.SAXParseException e) { errs.append(e.getMessage()).append("|"); }
                public void fatalError(org.xml.sax.SAXParseException e) { errs.append(e.getMessage()).append("|"); }
            });
            try {
                v.validate(new StreamSource(new ByteArrayInputStream(body)));
            } catch (Exception e) {
                if (errs.length() == 0) errs.append(e.getMessage());
            }
            String json = errs.length() == 0
                ? "{\"valid\":true}"
                : "{\"valid\":false,\"errors\":[\"" + errs.toString().replace("\"", "'").replace("\n"," ") + "\"]}";
            respond(ex, 200, json.getBytes(StandardCharsets.UTF_8));
        } catch (Exception e) {
            respond(ex, 500, ("{\"valid\":false,\"errors\":[\"oracle error\"]}").getBytes(StandardCharsets.UTF_8));
        }
    }

    private static void respond(com.sun.net.httpserver.HttpExchange ex, int code, byte[] body) {
        try (OutputStream os = ex.getResponseBody()) {
            ex.sendResponseHeaders(code, body.length);
            os.write(body);
        } catch (Exception ignored) {}
    }
}
```

> NOTE on body-payload validation: validating the *body child* against the controlled
> XSD (level §5.6.2) requires extracting that child first. For this plan, the orchestrator
> sends the **already-extracted body element bytes** (it has them from parsing) to
> `/validate?schema=controlled`, and the **whole envelope** to
> `/validate?schema=soap12-envelope`. The oracle just validates whatever bytes it is given
> against the named schema — keeping the oracle dumb and the orchestrator in control of
> which bytes go to which schema (matches §5.6's "validate envelope and payload separately").

- [ ] **Step 3: Bundle the schemas.** Create
  `crossref/comparators/oracle/src/main/resources/schemas/` and place:
  - `controlled.xsd` — the `<xs:schema>` extracted from `crossref/fixtures/controlled.wsdl`
    (the `targetNamespace="http://crossref.example/controlled"` schema with `Echo`,
    `EchoResponse`, `EchoNamed`*, `EchoNamedResponse`* elements). Copy it verbatim from the
    WSDL `<wsdl:types>` into a standalone `.xsd` (add the `<?xml?>` prolog and keep the
    `xmlns:xs` + `targetNamespace`).
  - `soap11-envelope.xsd` — fetch the canonical SOAP 1.1 envelope schema
    (`http://schemas.xmlsoap.org/soap/envelope/`) and vendor it here.
  - `soap12-envelope.xsd` — the SOAP 1.2 envelope schema
    (`http://www.w3.org/2003/05/soap-envelope`) vendored here. (The 1.2 envelope schema
    imports `xml.xsd`; vendor that too and wire `schemaLocation` to the local copy, or use
    an `LSResourceResolver` — simplest: vendor `xml.xsd` next to it and edit the import's
    `schemaLocation` to `xml.xsd`.)

> These XSDs are static, public W3C/OASIS artifacts. Vendor them (do not fetch at
> runtime). Document their source URLs in a `crossref/comparators/oracle/SCHEMAS.md`.

- [ ] **Step 4: `Dockerfile`** (multi-stage; builds entirely in-container — no host Java):

```dockerfile
# build stage
FROM maven:3.9-eclipse-temurin-21@sha256:PIN_ME AS build
WORKDIR /src
COPY pom.xml .
RUN mvn -q -e -o dependency:go-offline 2>/dev/null || mvn -q dependency:go-offline
COPY src ./src
RUN mvn -q package

# runtime stage
FROM eclipse-temurin:21-jre@sha256:PIN_ME
WORKDIR /app
COPY --from=build /src/target/oracle.jar /app/oracle.jar
EXPOSE 8081
ENTRYPOINT ["java", "-jar", "/app/oracle.jar"]
```

> Replace `PIN_ME` with the real digests: run `docker buildx imagetools inspect
> maven:3.9-eclipse-temurin-21` and `... eclipse-temurin:21-jre`, copy the
> `linux/amd64` (and arm64 if building locally on Apple Silicon — use a multi-arch digest
> or the index digest) `sha256`. Record the human-readable tag + digest in `manifest.toml`
> (Task 5).

- [ ] **Step 5: Build the oracle image and smoke-test it.**

```bash
docker build -t crossref-oracle:dev crossref/comparators/oracle
docker run -d --name ora -p 8081:8081 crossref-oracle:dev
sleep 2
# c14n smoke
curl -s -X POST localhost:8081/c14n --data-binary '<a   b="2" a="1">x</a>'
# validate smoke (valid SOAP 1.2 envelope)
curl -s -X POST 'localhost:8081/validate?schema=soap12-envelope' \
  --data-binary @crossref/scenarios/op_echo_success.request.xml
docker rm -f ora
```
Expected: c14n returns canonicalized bytes (attributes reordered to `a="1" b="2"`);
validate returns `{"valid":true}` for the well-formed envelope.

- [ ] **Step 6: Commit**

```bash
git add crossref/comparators/oracle
git commit -m "feat(crossref): containerised Java XML oracle (validate + exclusive C14N)"
```

---

## Task 4: CXF reference server container

**Files:**
- Create: `crossref/comparators/cxf/pom.xml`
- Create: `crossref/comparators/cxf/src/main/java/crossref/cxf/*.java`
- Create: `crossref/comparators/cxf/Dockerfile`

CXF publishes JAX-WS endpoints for the controlled Echo/EchoNamed services with
deterministic implementations; CXF owns the SOAP envelope/version/fault generation — that
is the reference signal we diff against.

- [ ] **Step 1: `pom.xml`** pinning CXF (JAX-WS + the embedded Jetty/CXF transport) and a
  shaded main.

```xml
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>crossref</groupId>
  <artifactId>cxf-ref</artifactId>
  <version>1.0.0</version>
  <packaging>jar</packaging>
  <properties>
    <maven.compiler.release>21</maven.compiler.release>
    <cxf.version>4.0.5</cxf.version>
  </properties>
  <dependencies>
    <dependency><groupId>org.apache.cxf</groupId><artifactId>cxf-rt-frontend-jaxws</artifactId><version>${cxf.version}</version></dependency>
    <dependency><groupId>org.apache.cxf</groupId><artifactId>cxf-rt-transports-http-jetty</artifactId><version>${cxf.version}</version></dependency>
  </dependencies>
  <build>
    <finalName>cxf-ref</finalName>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-shade-plugin</artifactId>
        <version>3.5.1</version>
        <executions><execution><phase>package</phase><goals><goal>shade</goal></goals>
          <configuration><transformers>
            <transformer implementation="org.apache.maven.plugins.shade.resource.ManifestResourceTransformer"><mainClass>crossref.cxf.Main</mainClass></transformer>
            <transformer implementation="org.apache.maven.plugins.shade.resource.ServicesResourceTransformer"/>
          </transformers></configuration>
        </execution></executions>
      </plugin>
    </plugins>
  </build>
</project>
```

- [ ] **Step 2: The SEI + impl + Main.** Define a JAX-WS service whose document/literal
  shape matches the controlled WSDL (`Echo` with required `Text`; `EchoNamed` with required
  `Value`). Use a code-first `@WebService` with `@XmlRootElement` request/response wrappers
  in the `http://crossref.example/controlled` namespace, deterministic impl, and a missing
  required element → a SOAP fault (CXF maps a thrown `SOAPFaultException` to a proper
  versioned fault).

```java
// Echo.java
package crossref.cxf;
import jakarta.jws.WebService;
import jakarta.jws.WebMethod;
import jakarta.jws.WebParam;

@WebService(targetNamespace = "http://crossref.example/controlled", name = "ControlledPort")
public interface Controlled {
    @WebMethod(operationName = "Echo")
    String echo(@WebParam(name = "Text", targetNamespace = "http://crossref.example/controlled") String text);

    @WebMethod(operationName = "EchoNamed")
    String echoNamed(@WebParam(name = "Value", targetNamespace = "http://crossref.example/controlled") String value);
}
```

```java
// ControlledImpl.java
package crossref.cxf;
import jakarta.jws.WebService;
import jakarta.xml.ws.soap.SOAPFaultException;
import jakarta.xml.soap.*;

@WebService(endpointInterface = "crossref.cxf.Controlled",
            targetNamespace = "http://crossref.example/controlled",
            serviceName = "ControlledService", portName = "ControlledPort")
public class ControlledImpl implements Controlled {
    public String echo(String text) {
        if (text == null) throw senderFault("required element 'Text' is missing");
        return text;
    }
    public String echoNamed(String value) {
        if (value == null) throw senderFault("required element 'Value' is missing");
        return value;
    }
    private SOAPFaultException senderFault(String reason) {
        try {
            SOAPFault f = SOAPFactory.newInstance(SOAPConstants.SOAP_1_2_PROTOCOL).createFault();
            f.setFaultCode(new javax.xml.namespace.QName(
                "http://www.w3.org/2003/05/soap-envelope", "Sender", "env"));
            f.setFaultString(reason);
            return new SOAPFaultException(f);
        } catch (SOAPException e) { throw new RuntimeException(e); }
    }
}
```

```java
// Main.java
package crossref.cxf;
import org.apache.cxf.jaxws.JaxWsServerFactoryBean;

public class Main {
    public static void main(String[] args) {
        JaxWsServerFactoryBean f = new JaxWsServerFactoryBean();
        f.setServiceClass(Controlled.class);
        f.setServiceBean(new ControlledImpl());
        f.setAddress("http://0.0.0.0:8082/soap");
        f.create();
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soap");
        try { Thread.currentThread().join(); } catch (InterruptedException ignored) {}
    }
}
```

> IMPORTANT — binding nuance: CXF code-first defaults to wrapped doc/literal. The
> controlled WSDL is bare doc/literal (`Echo`/`EchoResponse` elements directly). If CXF's
> generated request/response element names diverge (`Echo`/`echoResponse` casing, or a
> wrapper), the conformance diff will be `reference-disagreement`, NOT `sut-fail` — the
> verdict model handles this (Task 8). The implementer should first run the orchestrator
> against CXF, inspect the actual CXF response shape, and if it is *structurally
> equivalent but differently framed*, record it as a `known-divergence` (§5.7) with a
> path-scoped note rather than forcing byte-equality. Do NOT bend our server to match CXF;
> the controlled WSDL is the contract. If CXF cannot serve the bare doc/literal shape
> code-first, switch to **WSDL-first** (`wsdl2java` from `controlled.wsdl` in the build)
> and implement the generated SEI — note this in the report.

- [ ] **Step 3: `Dockerfile`** (multi-stage, in-container build):

```dockerfile
FROM maven:3.9-eclipse-temurin-21@sha256:PIN_ME AS build
WORKDIR /src
COPY pom.xml .
RUN mvn -q dependency:go-offline
COPY src ./src
RUN mvn -q package

FROM eclipse-temurin:21-jre@sha256:PIN_ME
WORKDIR /app
COPY --from=build /src/target/cxf-ref.jar /app/cxf-ref.jar
EXPOSE 8082
ENTRYPOINT ["java", "-jar", "/app/cxf-ref.jar"]
```

- [ ] **Step 4: Build + smoke-test** CXF echo:

```bash
docker build -t crossref-cxf:dev crossref/comparators/cxf
docker run -d --name cxf -p 8082:8082 crossref-cxf:dev
sleep 5
curl -s -X POST localhost:8082/soap -H 'content-type: application/soap+xml; charset=utf-8' \
  --data-binary @crossref/scenarios/op_echo_success.request.xml
docker rm -f cxf
```
Expected: a SOAP 1.2 envelope containing an echo of `hello` (exact framing is CXF's —
captured/normalized by the orchestrator, not asserted here).

- [ ] **Step 5: Commit**

```bash
git add crossref/comparators/cxf
git commit -m "feat(crossref): CXF reference server for controlled Echo/EchoNamed"
```

---

## Task 5: Comparator manifest + docker-compose topology

**Files:**
- Create: `crossref/manifest.toml`
- Create: `crossref/docker-compose.yml`

- [ ] **Step 1: `manifest.toml`** (§5.4) — digest-pin the base images actually used and
  record versions + scenario participation.

```toml
# Comparator registry (spec §5.4). Images pinned by immutable digest.
[[comparator]]
name = "java-xml-oracle"
role = "schema-oracle"
build = "comparators/oracle"
base_images = [
  "maven:3.9-eclipse-temurin-21@sha256:PIN_ME",
  "eclipse-temurin:21-jre@sha256:PIN_ME",
]
versions = { santuario = "4.0.3", jdk = "21" }
scenarios = ["*"]   # validates/canonicalizes every conformance scenario

[[comparator]]
name = "cxf"
role = "reference-server"
build = "comparators/cxf"
base_images = [
  "maven:3.9-eclipse-temurin-21@sha256:PIN_ME",
  "eclipse-temurin:21-jre@sha256:PIN_ME",
]
versions = { cxf = "4.0.5", jdk = "21" }
scenarios = [
  "op_echo_success", "op_echo_missing_required", "op_echo_empty_text",
  "op_echo_special_chars", "doc_literal_named_present", "doc_literal_named_missing",
  "ns_on_envelope", "ns_on_header", "ns_on_body", "ns_on_operation",
  "ns_on_nested_payload", "ns_prefix_shadowing",
]
```

- [ ] **Step 2: `docker-compose.yml`** — three services on a default network; healthchecks
  so the orchestrator can wait for readiness.

```yaml
name: crossref-layer2
services:
  controlled-server:
    build:
      context: ../..            # repo root (needs the whole crate to build the bin)
      dockerfile: crossref/comparators/controlled-server.Dockerfile
    expose: ["8080"]
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8080/soap?wsdl"]
      interval: 2s
      timeout: 2s
      retries: 30
  cxf:
    build: { context: ./comparators/cxf }
    expose: ["8082"]
    healthcheck:
      test: ["CMD", "bash", "-c", "echo > /dev/tcp/localhost/8082"]
      interval: 2s
      timeout: 2s
      retries: 30
  oracle:
    build: { context: ./comparators/oracle }
    expose: ["8081"]
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost:8081/healthz"]
      interval: 2s
      timeout: 2s
      retries: 30
```

- [ ] **Step 3: Create `crossref/comparators/controlled-server.Dockerfile`** (builds the
  Rust bin from the repo root context; multi-stage):

```dockerfile
FROM rust:1.88@sha256:PIN_ME AS build
WORKDIR /src
COPY . .
RUN cargo build -p crossref --bin controlled_server --release

FROM debian:bookworm-slim@sha256:PIN_ME
COPY --from=build /src/target/release/controlled_server /usr/local/bin/controlled_server
EXPOSE 8080
ENTRYPOINT ["controlled_server"]
```

- [ ] **Step 4: Bring the whole topology up and confirm all three become healthy.**

```bash
docker compose -f crossref/docker-compose.yml up -d --build
docker compose -f crossref/docker-compose.yml ps   # all "healthy"
docker compose -f crossref/docker-compose.yml down
```
Expected: `controlled-server`, `cxf`, `oracle` all reach `healthy`.

- [ ] **Step 5: Commit**

```bash
git add crossref/manifest.toml crossref/docker-compose.yml crossref/comparators/controlled-server.Dockerfile
git commit -m "feat(crossref): Layer-2 compose topology + comparator manifest"
```

---

## Task 6: Rust oracle client + compose lifecycle

**Files:**
- Create: `crossref/src/oracle.rs`
- Create: `crossref/src/layer2/mod.rs`, `crossref/src/layer2/compose.rs`
- Modify: `crossref/src/lib.rs` (`pub mod oracle; pub mod layer2;`), `crossref/Cargo.toml`
  (add `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`
  and `serde_json = "1"`).

- [ ] **Step 1: `oracle.rs`** — blocking HTTP client.

```rust
//! Rust client for the containerised Java XML oracle (validate + exclusive C14N).
//! The orchestrator NEVER validates or canonicalizes XML itself — it always delegates
//! to this oracle (spec §4.3).

pub struct Oracle {
    base: String,
    http: reqwest::blocking::Client,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl Oracle {
    pub fn new(base: impl Into<String>) -> Self {
        Oracle { base: base.into(), http: reqwest::blocking::Client::new() }
    }

    /// Exclusive-C14N the given XML bytes. Returns canonical bytes.
    pub fn c14n(&self, xml: &[u8]) -> Result<Vec<u8>, String> {
        let r = self.http.post(format!("{}/c14n", self.base))
            .body(xml.to_vec()).send().map_err(|e| e.to_string())?;
        if !r.status().is_success() {
            return Err(format!("c14n {}: {}", r.status(), r.text().unwrap_or_default()));
        }
        Ok(r.bytes().map_err(|e| e.to_string())?.to_vec())
    }

    /// Validate `xml` against the named schema id.
    pub fn validate(&self, xml: &[u8], schema: &str) -> Result<ValidationResult, String> {
        let r = self.http.post(format!("{}/validate?schema={}", self.base, schema))
            .body(xml.to_vec()).send().map_err(|e| e.to_string())?;
        let v: serde_json::Value = r.json().map_err(|e| e.to_string())?;
        Ok(ValidationResult {
            valid: v.get("valid").and_then(|b| b.as_bool()).unwrap_or(false),
            errors: v.get("errors").and_then(|e| e.as_array())
                .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        })
    }
}
```

- [ ] **Step 2: `layer2/compose.rs`** — shell out to `docker compose` (no heavy docker
  SDK dep) + readiness wait.

```rust
//! Layer-2 compose lifecycle: up (build) → wait healthy → down. Shells out to the
//! `docker` CLI to avoid a Docker SDK dependency in the published-adjacent crate.

use std::process::Command;
use std::path::Path;

const COMPOSE: &str = "crossref/docker-compose.yml";

pub struct Topology { down_on_drop: bool }

impl Topology {
    /// `docker compose up -d --build`, then block until all services are healthy.
    pub fn up(repo_root: &Path, keep_up: bool) -> Result<Self, String> {
        run(repo_root, &["compose", "-f", COMPOSE, "up", "-d", "--build"])?;
        wait_healthy(repo_root, &["controlled-server", "cxf", "oracle"], 120)?;
        Ok(Topology { down_on_drop: !keep_up })
    }
    pub fn down(repo_root: &Path) -> Result<(), String> {
        run(repo_root, &["compose", "-f", COMPOSE, "down", "-v"])
    }
}
impl Drop for Topology {
    fn drop(&mut self) {
        if self.down_on_drop {
            let _ = Topology::down(Path::new("."));
        }
    }
}

fn run(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new("docker").args(args).current_dir(dir).output()
        .map_err(|e| format!("docker: {e}"))?;
    if !out.status.success() {
        return Err(format!("docker {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

fn wait_healthy(dir: &Path, services: &[&str], max_secs: u64) -> Result<(), String> {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    loop {
        let mut all = true;
        for s in services {
            let out = Command::new("docker")
                .args(["compose", "-f", COMPOSE, "ps", "-q", s])
                .current_dir(dir).output().map_err(|e| e.to_string())?;
            let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if id.is_empty() { all = false; break; }
            let h = Command::new("docker")
                .args(["inspect", "-f", "{{.State.Health.Status}}", &id])
                .output().map_err(|e| e.to_string())?;
            if String::from_utf8_lossy(&h.stdout).trim() != "healthy" { all = false; break; }
        }
        if all { return Ok(()); }
        if start.elapsed() > Duration::from_secs(max_secs) {
            return Err("topology did not become healthy in time".into());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
```

> NOTE: `Command::new("docker")` / `sleep` may be sandbox-restricted. Layer 2 is intended
> to run unsandboxed (CI or an explicit local invocation), not in the per-commit gate.
> The `layer2` bin documents this.

- [ ] **Step 3: `layer2/mod.rs`** — wire submodules: `pub mod compose; pub mod verdict;
  pub mod promote; pub mod report;` and a `pub struct Endpoints { our: String, cxf: String,
  oracle: String }` with the in-network URLs (`http://controlled-server:8080/soap`,
  `http://cxf:8082/soap`, `http://oracle:8081`) for the CI/in-network case and a
  `localhost`-mapped variant for local runs (the bin chooses based on a `--local` flag,
  publishing ports if local).

> For local runs the compose file must publish ports; add a `docker-compose.local.yml`
> override mapping `8080:8080`, `8082:8082`, `8081:8081`, and have the `layer2 --local`
> bin pass `-f docker-compose.yml -f docker-compose.local.yml`. In CI the orchestrator runs
> as its own step on the host and talks to published ports too (simplest), so always use the
> override in this plan and target `localhost`.

- [ ] **Step 4: Add a unit test for the oracle client against a stub** (no Docker) — start
  a tiny in-process HTTP server in the test that returns canned `/c14n` + `/validate`
  responses and assert `Oracle::c14n`/`validate` parse them. Use `axum`/`tokio` already
  available.

```rust
// in crossref/src/oracle.rs tests
#[cfg(test)]
mod tests {
    use super::*;
    // Spin a tiny stub server returning fixed bodies; assert client parsing.
    // (Use std::net TcpListener + a hand-rolled HTTP/1.1 response, or axum on an ephemeral port.)
    #[test]
    fn parses_validation_json() { /* stub returns {"valid":false,"errors":["x"]}; assert */ }
}
```
Run: `cargo test -p crossref oracle::` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crossref/src/oracle.rs crossref/src/layer2 crossref/src/lib.rs crossref/Cargo.toml crossref/docker-compose.local.yml
git commit -m "feat(crossref): oracle HTTP client + Layer-2 compose lifecycle"
```

---

## Task 7: Verdict model + the conformance normalization pipeline

**Files:**
- Create: `crossref/src/layer2/verdict.rs`
- Modify: `crossref/src/normalize.rs` (expose a `mask_only` entry that masks the tree and
  re-serializes WITHOUT Rust canonicalization, so Layer 2 can hand masked bytes to the
  oracle for authoritative C14N).

- [ ] **Step 1: `normalize::mask_only`.** Add a function that runs the existing
  parse→path-scoped-mask→serialize pipeline but is explicitly the *masking* step for Layer
  2 (Layer 1 keeps using `normalize` for its self-contained regression serialize). They can
  share code; the point is a named entry that returns masked XML bytes for the oracle.

```rust
/// Mask path-scoped volatile fields and re-serialize. Layer-2 feeds the result to the
/// Java XML oracle for authoritative exclusive-C14N (this fn does NOT canonicalize).
pub fn mask_only(xml: &[u8], masks: &[MaskRule]) -> Result<Vec<u8>, String> {
    // identical traversal to `normalize`, returning bytes (not String); attribute order
    // is left as-is here because the oracle's C14N fixes ordering authoritatively.
    normalize(xml, masks).map(|s| s.into_bytes())
}
```
> If reusing `normalize` (which sorts attributes) is simplest, that's fine — the oracle
> re-canonicalizes regardless. The key property: masking is path-scoped and done in Rust;
> C14N is the oracle's.

- [ ] **Step 2: `verdict.rs`** — the §5.7 model + the per-scenario evaluation.

```rust
//! Spec §5.7 verdict model for a conformance scenario run.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    SutFail(String),
    ReferenceDisagreement(String),
    KnownDivergence(String),
    HarnessError(String),
}

/// Inputs already gathered by the orchestrator for one scenario.
pub struct Eval<'a> {
    pub our_valid: bool,
    pub our_errors: &'a [String],
    pub ref_valid: bool,
    pub ref_errors: &'a [String],
    /// oracle-canonical bytes of our masked response
    pub our_canon: &'a [u8],
    /// oracle-canonical bytes of the reference's masked response
    pub ref_canon: &'a [u8],
    /// a recorded allowed divergence for this scenario, if any
    pub known_divergence: Option<&'a str>,
}

pub fn evaluate(e: &Eval) -> Verdict {
    if !e.our_valid {
        return Verdict::SutFail(format!("our response schema-invalid: {:?}", e.our_errors));
    }
    if !e.ref_valid {
        // reference itself invalid → not a SUT verdict
        return Verdict::ReferenceDisagreement(
            format!("reference schema-invalid: {:?}", e.ref_errors));
    }
    if e.our_canon == e.ref_canon {
        return Verdict::Pass;
    }
    if let Some(reason) = e.known_divergence {
        return Verdict::KnownDivergence(reason.to_string());
    }
    // both schema-valid but canonical bytes differ
    Verdict::SutFail("our response disagrees with a schema-valid reference".into())
}
```

- [ ] **Step 3: Unit-test `evaluate`** for each branch (pass; our-invalid→SutFail;
  ref-invalid→ReferenceDisagreement; differ+known→KnownDivergence; differ→SutFail).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> (Vec<String>, Vec<String>) { (vec![], vec![]) }
    #[test] fn pass_when_valid_and_equal() {
        let (oe, re) = base();
        let v = evaluate(&Eval{our_valid:true,our_errors:&oe,ref_valid:true,ref_errors:&re,our_canon:b"x",ref_canon:b"x",known_divergence:None});
        assert_eq!(v, Verdict::Pass);
    }
    #[test] fn sutfail_when_our_invalid() {
        let oe = vec!["bad".into()]; let re: Vec<String> = vec![];
        let v = evaluate(&Eval{our_valid:false,our_errors:&oe,ref_valid:true,ref_errors:&re,our_canon:b"x",ref_canon:b"x",known_divergence:None});
        assert!(matches!(v, Verdict::SutFail(_)));
    }
    #[test] fn refdisagree_when_ref_invalid() {
        let oe: Vec<String> = vec![]; let re = vec!["bad".into()];
        let v = evaluate(&Eval{our_valid:true,our_errors:&oe,ref_valid:false,ref_errors:&re,our_canon:b"x",ref_canon:b"y",known_divergence:None});
        assert!(matches!(v, Verdict::ReferenceDisagreement(_)));
    }
    #[test] fn knowndiverge_when_differ_but_allowed() {
        let (oe,re)=base();
        let v = evaluate(&Eval{our_valid:true,our_errors:&oe,ref_valid:true,ref_errors:&re,our_canon:b"x",ref_canon:b"y",known_divergence:Some("CXF wraps differently")});
        assert!(matches!(v, Verdict::KnownDivergence(_)));
    }
    #[test] fn sutfail_when_differ_unexplained() {
        let (oe,re)=base();
        let v = evaluate(&Eval{our_valid:true,our_errors:&oe,ref_valid:true,ref_errors:&re,our_canon:b"x",ref_canon:b"y",known_divergence:None});
        assert!(matches!(v, Verdict::SutFail(_)));
    }
}
```
Run: `cargo test -p crossref verdict::` → PASS (5).

- [ ] **Step 4: Commit**

```bash
git add crossref/src/layer2/verdict.rs crossref/src/normalize.rs
git commit -m "feat(crossref): Layer-2 verdict model (spec 5.7) + mask-only entry"
```

---

## Task 8: Promotion + the orchestrator + report

**Files:**
- Create: `crossref/src/layer2/promote.rs`, `crossref/src/layer2/report.rs`
- Create: `crossref/src/bin/layer2.rs`
- Modify: `crossref/src/layer2/mod.rs` (the `run()` driver)

- [ ] **Step 1: `promote.rs`** — write the oracle-canonical golden + flip status.

```rust
//! Snapshot promotion (spec §5.2): on a Pass verdict, write the oracle-canonical bytes
//! as the golden snapshot and flip the scenario's status to `verified`.
use crate::snapshot::SnapshotStore;

pub fn promote(store: &SnapshotStore, name: &str, canonical: &[u8]) -> Result<(), String> {
    // Snapshot stores the oracle-canonical bytes (authoritative), replacing the
    // Layer-1 self-captured baseline.
    store.write_verified(name, std::str::from_utf8(canonical).map_err(|e| e.to_string())?)
}
```
Add `write_verified` + a `Status::Verified` write path to `snapshot.rs` (mirrors
`write_unverified` but writes `"verified"` to `status.toml`). Add a unit test in
`snapshot.rs` for `write_verified` round-trip.

- [ ] **Step 2: `report.rs`** — aggregate verdicts + still-`unverified` count (§5.2/§7).

```rust
//! Per-scenario verdict report. MUST surface the count of still-`unverified` snapshots
//! so self-captured baselines are never mistaken for conformance evidence.
use crate::layer2::verdict::Verdict;

pub struct Report { pub rows: Vec<(String, Verdict)>, pub unverified_remaining: usize }

impl Report {
    pub fn print(&self) {
        for (name, v) in &self.rows { println!("{name:40} {v:?}"); }
        println!("\n{} scenario(s); {} still unverified (conformance pending)",
                 self.rows.len(), self.unverified_remaining);
    }
    /// Exit non-zero if any SutFail/HarnessError present.
    pub fn is_green(&self) -> bool {
        !self.rows.iter().any(|(_, v)|
            matches!(v, Verdict::SutFail(_) | Verdict::HarnessError(_)))
    }
}
```

- [ ] **Step 3: `mod.rs::run()`** — the driver, gated to the conformance scenario set in
  the manifest. For each scenario:
  1. read request bytes;
  2. POST to our server and to CXF (in-network or localhost-published);
  3. for each response: `oracle.validate(envelope, "soap12-envelope")` AND
     extract the body child + `oracle.validate(body_child, "controlled")` (faults validate
     against the envelope's fault structure — for this plan, validating the full envelope
     against `soap12-envelope` covers fault structure at level §5.6.3);
  4. `normalize::mask_only` both → `oracle.c14n` both;
  5. `verdict::evaluate(...)`;
  6. on `Pass`, `promote::promote(store, name, our_canon)`;
  7. collect into `Report`.

  Provide a `known_divergences: HashMap<&str,&str>` seeded empty; the implementer fills it
  from the CXF-shape investigation (Task 4 Step 2 NOTE) so structural-but-framing-different
  scenarios are `KnownDivergence`, not `SutFail`.

```rust
pub fn run(endpoints: &Endpoints, repo_root: &std::path::Path, promote_on_pass: bool) -> Report {
    // (full driver: loop the manifest conformance scenarios, gather Eval, evaluate,
    //  optionally promote; build Report with unverified_remaining from the store.)
    // See steps above — implement straightforwardly; keep each helper small.
    unimplemented!("implement per Step 3 checklist")
}
```
> The implementer writes the concrete `run()` body per the numbered checklist; keep HTTP
> in a small `fn post(url, bytes, ct) -> (u16, Vec<u8>)` helper and body-child extraction
> reusing `soap_server::envelope` if exposed, else a small local quick-xml extract.

- [ ] **Step 4: `bin/layer2.rs`** — CLI.

```rust
//! Layer-2 entrypoint. Runs UNSANDBOXED (CI or explicit local). Not part of per-commit CI.
//! Usage: cargo run -p crossref --bin layer2 -- [--promote] [--keep-up]
use crossref::layer2::{compose::Topology, Endpoints, run};
use std::path::Path;

fn main() {
    let promote = std::env::args().any(|a| a == "--promote");
    let keep_up = std::env::args().any(|a| a == "--keep-up");
    let root = Path::new(".");
    let _topo = Topology::up(root, keep_up).expect("topology up");
    let endpoints = Endpoints::localhost(); // published ports via the local override
    let report = run(&endpoints, root, promote);
    report.print();
    std::process::exit(if report.is_green() { 0 } else { 1 });
}
```

- [ ] **Step 5: End-to-end local run (the real gate for this plan).**

```bash
docker compose -f crossref/docker-compose.yml -f crossref/docker-compose.local.yml up -d --build
cargo run -p crossref --bin layer2 -- --promote --keep-up
docker compose -f crossref/docker-compose.yml -f crossref/docker-compose.local.yml down -v
```
Expected: every in-scope conformance scenario resolves to `Pass` or a recorded
`KnownDivergence`; NO `SutFail`/`HarnessError`; the 12 in-scope scenarios flip to
`verified` in `crossref/snapshots/status.toml`; the report prints the still-`unverified`
remaining count (the 10 deferred scenarios).

> If a scenario is `SutFail`, that is a REAL finding — our server disagrees with CXF or is
> schema-invalid. STOP and report it (do not mask it away). Investigate whether it is our
> bug (fix in soap-server, like the Phase-1a envelope finding — surface to the user) or a
> CXF framing artifact (→ `KnownDivergence` with a documented reason).

- [ ] **Step 6: Verify Layer 1 still green + snapshots now verified.**

```bash
cargo test -p crossref --test layer1_replay   # diff still passes against promoted goldens
grep -c 'verified' crossref/snapshots/status.toml
```
Expected: Layer-1 replay PASSES against the newly promoted oracle-canonical snapshots (if
our canonical output differs from the old Rust-normalized snapshot, Layer 1's
`normalize` must produce the SAME canonical form — see NOTE below).

> CRITICAL reconciliation: Layer 1 diffs `normalize(our_response)` vs the snapshot. After
> promotion the snapshot is the ORACLE-canonical form. For Layer 1 to stay green, either
> (a) Layer 1 must compare using a canonicalization compatible with the oracle's, or
> (b) promotion stores BOTH a Layer-1 form and marks `verified`. Simplest correct choice
> for this plan: **Layer 1 keeps its own Rust-normalized snapshot; promotion records the
> `verified` STATUS (in status.toml) without overwriting the Layer-1 snapshot bytes**, and
> separately stores the oracle-canonical bytes under `snapshots/canonical/<name>.c14n` as
> the conformance evidence. This keeps Layer 1 fast/self-contained while `verified` status
> reflects Layer-2 promotion. IMPLEMENT THIS: `write_verified` flips status only; add
> `store.write_canonical(name, bytes)` for the oracle bytes. Adjust Task 8 Step 1
> accordingly and note it in the report.

- [ ] **Step 7: Commit**

```bash
git add crossref/src/layer2 crossref/src/bin/layer2.rs crossref/src/snapshot.rs crossref/snapshots
git commit -m "feat(crossref): Layer-2 orchestrator — drive, validate, diff, verdict, promote"
```

---

## Task 9: Layer-2 CI workflow + README

**Files:**
- Create: `.github/workflows/layer2.yml`
- Modify: `crossref/README.md`

- [ ] **Step 1: `.github/workflows/layer2.yml`** — Linux + Docker, on `workflow_dispatch`
  and a nightly `schedule`; NOT on push (keeps the per-commit gate fast).

```yaml
name: crossref Layer 2 (conformance)
on:
  workflow_dispatch:
  schedule:
    - cron: "0 13 * * *"   # nightly
jobs:
  layer2:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with: { toolchain: "1.88.0" }
      - uses: Swatinem/rust-cache@v2
      - name: Build images + run Layer 2
        run: |
          docker compose -f crossref/docker-compose.yml -f crossref/docker-compose.local.yml up -d --build
          cargo run -p crossref --bin layer2 -- --promote
      - name: Tear down
        if: always()
        run: docker compose -f crossref/docker-compose.yml -f crossref/docker-compose.local.yml down -v
      - name: Surface snapshot drift / promotion as reviewable change
        run: |
          git status --short crossref/snapshots
          # Layer 2 promotion changes are committed deliberately, not by CI; this step
          # only surfaces them. A non-empty diff here on a scheduled run is expected
          # (newly verified) and reviewed via a follow-up PR, never auto-committed.
```

- [ ] **Step 2: README** — add a "Layer 2 (Docker conformance)" section: prerequisites
  (Docker only — Java is fully containerised), how to run locally
  (`docker compose … up --build` + `cargo run --bin layer2 -- --promote`), what `verified`
  means, and that WS-Security/multi-service/SOAP-1.1 conformance + interop are Phase 1c.

- [ ] **Step 3: Final gates.**

```bash
cargo test --workspace --all-features     # Layer 1 + unit tests green (Layer 2 bin not run here)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/layer2.yml')); print('yaml ok')"
```
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/layer2.yml crossref/README.md
git commit -m "ci(crossref): nightly/on-demand Layer-2 conformance workflow + README"
```

---

## Self-review notes (author)

- **Spec coverage:** §4.2 Layer 2 (Tasks 6–8), §4.3 containerised authorities (Tasks 3,4 —
  Java fully in-container, host needs only Docker+cargo), §5.2 promotion (Task 8 + the
  Layer-1 reconciliation NOTE), §5.4 manifest (Task 5), §5.6 validation levels (Task 3
  oracle + Task 8 driver: envelope schema + body-child schema; fault structure via
  envelope schema), §5.7 verdict model (Task 7), §6 layout (matches `crossref/` tree),
  §7 CI Layer-2 workflow (Task 9), §8 phase 1b (this plan).
- **Deferred & reported (NOT silently dropped):** WS-Security conformance, multi-service &
  SOAP 1.1 conformance, all interop (1c). These stay `unverified`; the report surfaces the
  count (§5.2 honesty requirement). A Phase 1c plan covers them.
- **Known implementer risks flagged inline:** (1) CXF code-first doc/literal framing may
  diverge from the bare controlled WSDL → use `KnownDivergence` or switch to WSDL-first
  (Task 4 NOTE); (2) Layer-1↔oracle canonical-form reconciliation → promotion flips status
  only + stores oracle bytes separately (Task 8 Step 6 NOTE); (3) `docker`/`sleep` are
  sandbox-sensitive → Layer 2 runs unsandboxed (Task 6 NOTE); (4) base-image digests must
  be filled in (`PIN_ME`) before building (Tasks 3,4,5).
- **Verdict honesty:** `SutFail` is a real finding to surface to the user (like the
  Phase-1a envelope bug), never masked; `reference-disagreement` never auto-passes/fails
  the SUT; `harness-error` never counts as pass.
- **Publish safety:** everything added is under `crossref/` (already `publish=false` +
  excluded from the soap-server tarball) or `.github/`; `cargo publish` of soap-server
  stays unaffected (re-verify with `cargo package --list -p soap-server | grep -c '^crossref/'`).
