package crossref.cxf;

import jakarta.xml.ws.Endpoint;
import jakarta.xml.ws.soap.SOAPBinding;

public class Main {
    public static void main(String[] args) throws Exception {
        // SOAPBinding.SOAP12HTTP_BINDING = "http://www.w3.org/2003/05/soap/bindings/HTTP/"
        Endpoint ep = Endpoint.create(SOAPBinding.SOAP12HTTP_BINDING, new ControlledImpl());
        ep.publish("http://0.0.0.0:8082/soap");
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soap");
        Thread.currentThread().join();
    }
}
