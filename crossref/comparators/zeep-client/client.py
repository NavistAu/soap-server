"""
Zeep interop client: drives our controlled server's Echo operation.

Loads the controlled WSDL from http://controlled-server:8080/soap?wsdl,
calls the Echo operation with Text="interop_zeep", asserts the result
echoes "interop_zeep", prints the response, exits 0 on success, 1 on failure.

Our WSDL is doc/literal SOAP 1.2 (xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap12/").
The WSDL binding name is "ControlledBinding", service "ControlledService",
port "ControlledPort".

Zeep note: Zeep auto-selects the service/port from the WSDL.
If Zeep defaults to SOAP 1.1 lookup, we explicitly create_service() with
the correct binding name and endpoint address.
"""

import os
import sys
import traceback

# Unset proxy environment variables before importing requests/zeep.
# OrbStack injects HTTP_PROXY/http_proxy into all containers, which routes
# internal compose-network traffic (e.g. controlled-server) through an external
# proxy that returns 502. Clearing these ensures direct internal routing.
for _proxy_var in (
    "http_proxy", "HTTP_PROXY",
    "https_proxy", "HTTPS_PROXY",
    "all_proxy", "ALL_PROXY",
):
    os.environ.pop(_proxy_var, None)

WSDL_URL = "http://controlled-server:8080/soap?wsdl"
ENDPOINT  = "http://controlled-server:8080/soap"
BINDING   = "{http://crossref.example/controlled}ControlledBinding"
TEXT      = "interop_zeep"


def main():
    print(f"Zeep interop client starting", file=sys.stderr)
    print(f"  WSDL: {WSDL_URL}", file=sys.stderr)

    try:
        from zeep import Client, Settings
        from zeep.transports import Transport
        import requests

        session = requests.Session()
        # Disable proxy for all requests: the compose network is private and any
        # HTTP_PROXY / http_proxy env vars injected by the container runtime (e.g.
        # OrbStack's proxyproxy.orb.internal) must not intercept internal traffic.
        session.proxies = {"http": None, "https": None}
        transport = Transport(session=session)
        settings = Settings(strict=False, xml_huge_tree=True)

        print("Loading WSDL ...", file=sys.stderr)
        client = Client(WSDL_URL, transport=transport, settings=settings)

        # Log what services/bindings Zeep found.
        print(f"WSDL loaded. Services: {[str(s) for s in client.wsdl.services.keys()]}", file=sys.stderr)

        # Explicitly create a service proxy with our binding and endpoint address,
        # avoiding any default SOAP 1.1 vs 1.2 resolution ambiguity.
        try:
            service = client.create_service(BINDING, ENDPOINT)
            print(f"Using explicit binding: {BINDING}", file=sys.stderr)
        except Exception as e:
            # Fallback: use the default service (Zeep may auto-detect correctly).
            print(f"create_service failed ({e}), falling back to default service", file=sys.stderr)
            service = client.service

        print(f"Calling Echo(Text={TEXT!r}) ...", file=sys.stderr)
        result = service.Echo(Text=TEXT)
        print(f"Raw result: {result!r}", file=sys.stderr)

        # Result is a zeep object; serialize to string for the assertion check.
        # Zeep returns the deserialized Python value — for doc/literal the response
        # is a zeep object with a Text attribute.
        result_str = str(result)

        # Also capture the raw XML response for the orchestrator.
        # Zeep doesn't expose the raw XML easily via the high-level API; print what we have.
        print(result_str)

        # Assert the response contains the echo text.
        if TEXT not in result_str:
            print(f"ASSERTION FAILED: result {result_str!r} does not contain {TEXT!r}", file=sys.stderr)
            sys.exit(1)

        print(f"PASS: Echo round-trip successful, result contains {TEXT!r}", file=sys.stderr)
        sys.exit(0)

    except Exception as e:
        print(f"FATAL: {e}", file=sys.stderr)
        traceback.print_exc(file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
