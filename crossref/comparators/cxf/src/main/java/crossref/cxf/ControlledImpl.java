package crossref.cxf;

import crossref.cxf.generated.ControlledPort;

import jakarta.jws.WebService;
import jakarta.xml.ws.Holder;
import jakarta.xml.ws.soap.SOAPFaultException;
import jakarta.xml.soap.*;
import javax.xml.namespace.QName;

/**
 * Deterministic WSDL-first implementation of the ControlledPort SEI.
 * Bare doc/literal: CXF generates Holder parameters for in-out fields.
 */
@WebService(endpointInterface = "crossref.cxf.generated.ControlledPort",
            targetNamespace = "http://crossref.example/controlled",
            serviceName = "ControlledService",
            portName = "ControlledPort")
public class ControlledImpl implements ControlledPort {

    @Override
    public void echo(Holder<String> text) {
        if (text == null || text.value == null) {
            throw senderFault("required element 'Text' is missing");
        }
        // echo: value stays as-is (in-out holder, already set to the input)
    }

    @Override
    public void echoNamed(Holder<String> value) {
        if (value == null || value.value == null) {
            throw senderFault("required element 'Value' is missing");
        }
        // echo: value stays as-is (in-out holder, already set to the input)
    }

    @Override
    public String faulty(String trigger) {
        // Always throws a Sender fault with a raw XML detail child:
        // <c:ErrorInfo xmlns:c="http://crossref.example/controlled">
        //   <c:Field>missing-text</c:Field>
        // </c:ErrorInfo>
        try {
            SOAPFault f = SOAPFactory.newInstance(SOAPConstants.SOAP_1_2_PROTOCOL).createFault();
            f.setFaultCode(new QName(
                "http://www.w3.org/2003/05/soap-envelope", "Sender", "env"));
            f.setFaultString("operation failed");

            Detail detail = f.addDetail();
            String controlledNs = "http://crossref.example/controlled";
            DetailEntry entry = detail.addDetailEntry(new QName(controlledNs, "ErrorInfo", "c"));
            entry.addNamespaceDeclaration("c", controlledNs);
            SOAPElement field = entry.addChildElement(new QName(controlledNs, "Field", "c"));
            field.setTextContent("missing-text");

            throw new SOAPFaultException(f);
        } catch (SOAPException e) {
            throw new RuntimeException(e);
        }
    }

    private SOAPFaultException senderFault(String reason) {
        // Try to detect the current endpoint's SOAP version from the message context.
        // Falls back to SOAP 1.1 if unavailable (works for both 1.1 and 1.2 endpoints
        // because CXF's fault interceptor re-serialises using the endpoint's binding).
        try {
            // Attempt SOAP 1.2 fault first (for the /soap endpoint).
            SOAPFault f12 = SOAPFactory.newInstance(SOAPConstants.SOAP_1_2_PROTOCOL).createFault();
            f12.setFaultCode(new javax.xml.namespace.QName(
                "http://www.w3.org/2003/05/soap-envelope", "Sender", "env"));
            f12.setFaultString(reason);
            return new SOAPFaultException(f12);
        } catch (SOAPException e) {
            throw new RuntimeException(e);
        }
    }

    /** SOAP 1.1 sender fault — used by the /soap11 endpoint via {@link ControlledImpl11}. */
    static SOAPFaultException senderFault11(String reason) {
        try {
            SOAPFault f = SOAPFactory.newInstance(SOAPConstants.SOAP_1_1_PROTOCOL).createFault();
            f.setFaultCode(new javax.xml.namespace.QName(
                "http://schemas.xmlsoap.org/soap/envelope/", "Client", "env"));
            f.setFaultString(reason);
            return new SOAPFaultException(f);
        } catch (SOAPException e) {
            throw new RuntimeException(e);
        }
    }
}
