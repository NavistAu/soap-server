package crossref.cxf;

import jakarta.xml.ws.Endpoint;
import jakarta.xml.ws.soap.SOAPBinding;
import org.apache.cxf.jaxws.JaxWsServerFactoryBean;

public class Main {
    public static void main(String[] args) throws Exception {
        // SOAP 1.2 endpoint (existing).
        // SOAPBinding.SOAP12HTTP_BINDING = "http://www.w3.org/2003/05/soap/bindings/HTTP/"
        Endpoint ep12 = Endpoint.create(SOAPBinding.SOAP12HTTP_BINDING, new ControlledImpl());
        ep12.publish("http://0.0.0.0:8082/soap");
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soap (SOAP 1.2)");

        // SOAP 1.1 endpoint — same implementation, different binding.
        // SOAPBinding.SOAP11HTTP_BINDING = "http://schemas.xmlsoap.org/wsdl/soap/http"
        JaxWsServerFactoryBean sf11 = new JaxWsServerFactoryBean();
        sf11.setServiceClass(crossref.cxf.generated.ControlledPort.class);
        sf11.setServiceBean(new ControlledImpl11());
        sf11.setBindingId(SOAPBinding.SOAP11HTTP_BINDING);
        sf11.setAddress("http://0.0.0.0:8082/soap11");
        sf11.create();
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soap11 (SOAP 1.1)");

        Thread.currentThread().join();
    }
}
