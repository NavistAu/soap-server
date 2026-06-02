package crossref.cxf;

import crossref.cxf.generated.ControlledPort;

import jakarta.jws.WebService;
import jakarta.xml.ws.Holder;
import jakarta.xml.ws.soap.SOAPFaultException;
import jakarta.xml.soap.*;

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

    private SOAPFaultException senderFault(String reason) {
        try {
            SOAPFault f = SOAPFactory.newInstance(SOAPConstants.SOAP_1_2_PROTOCOL).createFault();
            f.setFaultCode(new javax.xml.namespace.QName(
                "http://www.w3.org/2003/05/soap-envelope", "Sender", "env"));
            f.setFaultString(reason);
            return new SOAPFaultException(f);
        } catch (SOAPException e) {
            throw new RuntimeException(e);
        }
    }
}
