# Changelog

## [0.2.0](https://github.com/NavistAu/soap-server/compare/v0.1.0...v0.2.0) (2026-06-01)


### Features

* **01-01:** add module skeleton and ONVIF test fixtures ([cfee9f5](https://github.com/NavistAu/soap-server/commit/cfee9f58bb0ea56a574f5ccbe10e254511a1d4e2))
* **01-02:** add XSD and WSDL data type definitions with unit tests ([42179fa](https://github.com/NavistAu/soap-server/commit/42179fa9508d6c93b03a0e32fc3de96d1082b73b))
* **01-02:** implement SoapFault, FaultCode, and SoapHandler trait with tests ([bb2af4a](https://github.com/NavistAu/soap-server/commit/bb2af4afcca7b920fe2554b396a1252b5115dfa7))
* **01-03:** XSD Pass 1 parser — all visit_* functions and RawSchema ([3623721](https://github.com/NavistAu/soap-server/commit/362372137baa78c1b9caf755c6defa080d7892b7))
* **01-04:** implement SOAP envelope parse, serialize, and version detection ([a4df466](https://github.com/NavistAu/soap-server/commit/a4df46690cffe512e166acf617139bf20ea7cc9d))
* **01-04:** implement WS-Security timestamp validator and nonce replay cache ([045cd5a](https://github.com/NavistAu/soap-server/commit/045cd5ae42ff43e54242ce5dd3951690b7a6d0d1))
* **01-05:** implement XSD Pass 2 resolver with extension chain flattening and cycle detection ([8c08d4e](https://github.com/NavistAu/soap-server/commit/8c08d4ed5030035ada31327c2ab5b1998c3d059e))
* **01-06:** WS-Security UsernameToken validation ([99a5aeb](https://github.com/NavistAu/soap-server/commit/99a5aebcba4fdab82496e9c6afd4b82001e9ea3a))
* **01-06:** WSDL Pass 1 parser — all WSDL 1.1 constructs ([57ba7b9](https://github.com/NavistAu/soap-server/commit/57ba7b9109631ab10a62bb64c420f96b8f826ebc))
* **01-07:** WSDL Pass 2 resolver — cross-ref wiring, import merging, schema delegation, address rewrite ([ef52590](https://github.com/NavistAu/soap-server/commit/ef5259092a9c58c9138be7c548e382ff5291819b))
* **01-08:** implement DispatchTable, build_dispatch_table, route, validate_request ([af172fe](https://github.com/NavistAu/soap-server/commit/af172fed304d6c86aa43e32765026df3194d14df))
* **01-09:** ServerBuilder, SoapService, and request pipeline ([8a8afef](https://github.com/NavistAu/soap-server/commit/8a8afefe46092b9398bc653defc2bf17a142b214))
* **01-10:** ONVIF end-to-end integration tests (Phase 1 acceptance gate) ([7fc7ec3](https://github.com/NavistAu/soap-server/commit/7fc7ec35b3d01e81460f7a16abe8d72c0b15f5ee))
* **02-01:** add SOAP 1.1 envelope unit tests ([0bd1792](https://github.com/NavistAu/soap-server/commit/0bd17927603d9ee0ecf44af5de5fbce9f0dd49e7))
* **02-02:** add SOAP 1.1 fault serializer to fault.rs ([d97d4e1](https://github.com/NavistAu/soap-server/commit/d97d4e1d3633e078f829b87529186cc4baeb8a1a))
* **02-02:** wire versioned fault serializer and add SOAP 1.1 integration tests ([7f86e91](https://github.com/NavistAu/soap-server/commit/7f86e919865fb99b8e8b7a631e7f727b8b47f05d))
* **02-03:** add multi-service routing and RPC integration tests ([ca3d495](https://github.com/NavistAu/soap-server/commit/ca3d4951ba394468490bc1d004b767f2a8467af5))
* **02-03:** add RPC dispatch QName synthesis and per-service build function ([bd1853f](https://github.com/NavistAu/soap-server/commit/bd1853feaa80b8a2d4706a2e64224fdedee890a3))
* **03-01:** add WSDL GET handler to multi-service into_router() branch ([48375c1](https://github.com/NavistAu/soap-server/commit/48375c193d5126fccd0307b37f971322ecfd8491))
* **03-01:** promote internal types to public API and remove stale TODOs ([9c2c639](https://github.com/NavistAu/soap-server/commit/9c2c63987c45180a5de1dfc7524cd39b4752d5cf))
* **04-01:** add multi-service WSDL GET integration test; fix 03-02-SUMMARY.md ([c97661a](https://github.com/NavistAu/soap-server/commit/c97661aa55379c171824c56848bed7d07b4d57b4))
* expose SOAP headers to handlers via handle_with_headers (round-2 [#5](https://github.com/NavistAu/soap-server/issues/5)) ([31f03c6](https://github.com/NavistAu/soap-server/commit/31f03c60636ac15e690e323bcb13002afc3b192b))


### Bug Fixes

* **01:** remove pipe violations and misleading WSDL-01 claim ([de50ee4](https://github.com/NavistAu/soap-server/commit/de50ee46ab5517cd7f018b81d4946a08905f459f))
* **02-01:** fault_response() now accepts SoapVersion for correct Content-Type ([39654ab](https://github.com/NavistAu/soap-server/commit/39654abf83818fbaa951c77c2fb038899e4bf7d7))
* **04-01:** use MatchedPath extractor in wsdl_get_handler for per-service URL ([e9d8928](https://github.com/NavistAu/soap-server/commit/e9d892849a4774d213a4a6c5af755c80ea8ca3c1))
* address round-1 review blockers ([22a1800](https://github.com/NavistAu/soap-server/commit/22a1800c0d10e31be4a97e77c7250d3fdbeced90))
* commit remaining clippy auto-fixes (dispatch, envelope) ([fd87cb2](https://github.com/NavistAu/soap-server/commit/fd87cb2c7e2b12fc3cf809eb2a12d3e20b59bfae))
* resolve all clippy -D warnings issues to pass CI check ([a7e0bb0](https://github.com/NavistAu/soap-server/commit/a7e0bb04f305bb158377a9b79fcf56ed8421a822))
* round-2 review — namespace scope preservation, doc/literal XSD validation, per-service WSDL rewrite ([dc37be0](https://github.com/NavistAu/soap-server/commit/dc37be0201f846b54b22390472553a320d88c1a6))
* round-2 review — XML escaping, WS-Security hardening, DoS limits, error hygiene ([3d849de](https://github.com/NavistAu/soap-server/commit/3d849dea764940b1dbacdb8c931011e47ee81cb0))
* round-2 review [#4](https://github.com/NavistAu/soap-server/issues/4) — support unauthenticated mode ([b4bb4d2](https://github.com/NavistAu/soap-server/commit/b4bb4d29f7107c6b15de2a54efea96e7dd900d16))
