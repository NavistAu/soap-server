package crossref.cxf;

import crossref.cxf.generated.ControlledPort;

import jakarta.jws.WebService;
import jakarta.xml.ws.Holder;
import jakarta.xml.ws.soap.SOAPFaultException;
import jakarta.xml.soap.*;

/**
 * SOAP 1.1 implementation of ControlledPort. Raises proper SOAP 1.1 faults
 * (faultcode / faultstring, no xml:lang on faultstring) instead of SOAP 1.2
 * Code/Reason/Text. Published at /soap11 by Main.
 */
@WebService(endpointInterface = "crossref.cxf.generated.ControlledPort",
            targetNamespace = "http://crossref.example/controlled",
            serviceName = "ControlledService",
            portName = "ControlledPort")
public class ControlledImpl11 implements ControlledPort {

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

    private SOAPFaultException senderFault(String reason) {
        try {
            SOAPFault f = SOAPFactory.newInstance(SOAPConstants.SOAP_1_1_PROTOCOL).createFault();
            // SOAP 1.1 fault code: env:Client (no namespace in the local name itself,
            // but prefixed with the SOAP 1.1 envelope namespace prefix).
            f.setFaultCode(new javax.xml.namespace.QName(
                "http://schemas.xmlsoap.org/soap/envelope/", "Client", "SOAP-ENV"));
            f.setFaultString(reason);
            return new SOAPFaultException(f);
        } catch (SOAPException e) {
            throw new RuntimeException(e);
        }
    }
}
