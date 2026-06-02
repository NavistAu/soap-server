package crossref.cxfclient;

import javax.xml.namespace.QName;
import javax.xml.transform.Source;
import javax.xml.transform.stream.StreamSource;
import jakarta.xml.ws.Dispatch;
import jakarta.xml.ws.Service;
import jakarta.xml.ws.soap.SOAPBinding;
import java.io.ByteArrayInputStream;
import java.io.StringWriter;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import javax.xml.transform.Transformer;
import javax.xml.transform.TransformerFactory;
import javax.xml.transform.stream.StreamResult;

/**
 * CXF interop client: drives our controlled server's Echo operation via raw Dispatch.
 *
 * Uses Service/Dispatch<Source> (raw payload dispatch) targeting
 * http://controlled-server:8080/soap. Sends Echo with Text="interop_cxf", reads the
 * response, asserts the response contains "interop_cxf" in an EchoResponse.
 * Prints the raw response envelope to stdout.
 * System.exit(0) on success, 1 on any failure/assertion error.
 */
public class Client {
    private static final String WSDL_URL    = "http://controlled-server:8080/soap?wsdl";
    private static final String ENDPOINT    = "http://controlled-server:8080/soap";
    private static final String NS          = "http://crossref.example/controlled";
    private static final String SERVICE_NAME = "ControlledService";
    private static final String PORT_NAME   = "ControlledPort";

    // SOAP 1.2 envelope request
    private static final String SOAP12_REQUEST =
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>" +
        "<env:Envelope xmlns:env=\"http://www.w3.org/2003/05/soap-envelope\"" +
        "              xmlns:tns=\"http://crossref.example/controlled\">" +
        "  <env:Body>" +
        "    <tns:Echo><tns:Text>interop_cxf</tns:Text></tns:Echo>" +
        "  </env:Body>" +
        "</env:Envelope>";

    public static void main(String[] args) {
        try {
            System.err.println("CXF interop client starting");
            System.err.println("  WSDL: " + WSDL_URL);
            System.err.println("  Endpoint: " + ENDPOINT);

            // Build a Service from the live WSDL.
            QName serviceQName = new QName(NS, SERVICE_NAME);
            QName portQName    = new QName(NS, PORT_NAME);
            URL wsdlUrl = new URL(WSDL_URL);

            Service service = Service.create(wsdlUrl, serviceQName);

            // Create a raw Source dispatch (MESSAGE mode = full envelope).
            Dispatch<Source> dispatch = service.createDispatch(
                portQName,
                Source.class,
                Service.Mode.MESSAGE
            );

            // Force SOAP 1.2 binding on the dispatch.
            ((SOAPBinding) dispatch.getBinding()).setMTOMEnabled(false);

            // Set endpoint address explicitly (in case WSDL has placeholder).
            dispatch.getRequestContext().put(
                "javax.xml.ws.service.endpoint.address", ENDPOINT
            );
            // SOAP Action for Echo.
            dispatch.getRequestContext().put(
                "javax.xml.ws.soap.action",
                "http://crossref.example/controlled/Echo"
            );

            System.err.println("Sending Echo request with Text=interop_cxf ...");

            // Send the raw SOAP 1.2 envelope.
            Source requestSource = new StreamSource(
                new ByteArrayInputStream(SOAP12_REQUEST.getBytes(StandardCharsets.UTF_8))
            );

            Source responseSource = dispatch.invoke(requestSource);

            // Convert the response Source to a String.
            Transformer transformer = TransformerFactory.newInstance().newTransformer();
            StringWriter sw = new StringWriter();
            transformer.transform(responseSource, new StreamResult(sw));
            String responseXml = sw.toString();

            // Print raw response to stdout (the orchestrator captures this).
            System.out.println(responseXml);

            System.err.println("Response received (" + responseXml.length() + " chars)");

            // Assert the response contains the echo text.
            if (!responseXml.contains("interop_cxf")) {
                System.err.println("ASSERTION FAILED: response does not contain 'interop_cxf'");
                System.err.println("Response was: " + responseXml);
                System.exit(1);
            }

            // Assert the response contains EchoResponse element.
            if (!responseXml.contains("EchoResponse")) {
                System.err.println("ASSERTION FAILED: response does not contain 'EchoResponse'");
                System.err.println("Response was: " + responseXml);
                System.exit(1);
            }

            System.err.println("PASS: Echo round-trip successful, response contains 'interop_cxf' in EchoResponse");
            System.exit(0);

        } catch (Exception e) {
            System.err.println("FATAL: " + e.getMessage());
            e.printStackTrace(System.err);
            System.exit(1);
        }
    }
}
