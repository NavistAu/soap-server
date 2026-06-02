package crossref.oracle;

import com.sun.net.httpserver.HttpServer;
import org.apache.xml.security.Init;
import org.apache.xml.security.c14n.Canonicalizer;
import org.w3c.dom.ls.LSInput;
import org.w3c.dom.ls.LSResourceResolver;

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

    /**
     * LSResourceResolver that resolves schema imports from the classpath /schemas/ directory.
     * Required so that soap12-envelope.xsd can import xml.xsd by relative name.
     */
    private static class ClasspathSchemaResolver implements LSResourceResolver {
        public LSInput resolveResource(String type, String namespaceURI, String publicId,
                                       String systemId, String baseURI) {
            // Map well-known namespace URIs to vendored classpath resources
            String resource = null;
            if ("http://www.w3.org/XML/1998/namespace".equals(namespaceURI)
                    || "xml.xsd".equals(systemId)
                    || (systemId != null && systemId.contains("xml.xsd"))) {
                resource = "/schemas/xml.xsd";
            } else if (systemId != null && systemId.endsWith("soap12-envelope.xsd")) {
                resource = "/schemas/soap12-envelope.xsd";
            } else if (systemId != null && systemId.endsWith("soap11-envelope.xsd")) {
                resource = "/schemas/soap11-envelope.xsd";
            }
            if (resource == null) return null;
            final String res = resource;
            return new LSInput() {
                public java.io.Reader getCharacterStream() { return null; }
                public void setCharacterStream(java.io.Reader r) {}
                public InputStream getByteStream() { return Oracle.class.getResourceAsStream(res); }
                public void setByteStream(InputStream i) {}
                public String getStringData() { return null; }
                public void setStringData(String s) {}
                public String getSystemId() { return systemId; }
                public void setSystemId(String s) {}
                public String getPublicId() { return publicId; }
                public void setPublicId(String s) {}
                public String getBaseURI() { return baseURI; }
                public void setBaseURI(String s) {}
                public String getEncoding() { return "UTF-8"; }
                public void setEncoding(String s) {}
                public boolean getCertifiedText() { return false; }
                public void setCertifiedText(boolean b) {}
            };
        }
    }

    public static void main(String[] args) throws Exception {
        Init.init(); // Santuario
        SchemaFactory sf = SchemaFactory.newInstance(XMLConstants.W3C_XML_SCHEMA_NS_URI);
        sf.setResourceResolver(new ClasspathSchemaResolver());
        // Schemas are bundled on the classpath under /schemas/. Each maps an id -> .xsd.
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
