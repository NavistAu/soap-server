package crossref.cxf;

import jakarta.xml.ws.Endpoint;
import jakarta.xml.ws.soap.SOAPBinding;
import org.apache.cxf.jaxws.JaxWsServerFactoryBean;
import org.apache.cxf.ws.security.wss4j.WSS4JInInterceptor;
import org.apache.wss4j.dom.handler.WSHandlerConstants;

import java.util.HashMap;
import java.util.Map;

public class Main {
    public static void main(String[] args) throws Exception {
        // SOAP 1.2 endpoint (existing, unauthed).
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

        // WS-Security (WSS4J) secured SOAP 1.2 endpoint — LENIENT timestamp TTL.
        // Accepts the fixed Created=2020-01-01 in the test fixtures by setting an
        // enormous TTL (10 years = 315360000 seconds). The UsernameToken PasswordDigest
        // is verified by WSS4J using the password supplied by PasswordCallbackHandler.
        JaxWsServerFactoryBean sfSec = new JaxWsServerFactoryBean();
        sfSec.setServiceClass(crossref.cxf.generated.ControlledPort.class);
        sfSec.setServiceBean(new ControlledImpl());
        sfSec.setBindingId(SOAPBinding.SOAP12HTTP_BINDING);
        sfSec.setAddress("http://0.0.0.0:8082/soapsec");

        Map<String, Object> lenientProps = new HashMap<>();
        lenientProps.put(WSHandlerConstants.ACTION, WSHandlerConstants.USERNAME_TOKEN);
        lenientProps.put(WSHandlerConstants.PASSWORD_TYPE, "PasswordDigest");
        lenientProps.put(WSHandlerConstants.PW_CALLBACK_CLASS, PasswordCallbackHandler.class.getName());
        // TTL for UsernameToken Created freshness: 10 years (seconds).
        // WSS4J uses "ttl" for how far in the past a token's Created can be.
        lenientProps.put(WSHandlerConstants.TTL_USERNAMETOKEN, "315360000");
        // futureTimeToLive: how far in the future Created can be.
        lenientProps.put(WSHandlerConstants.TTL_FUTURE_USERNAMETOKEN, "315360000");
        // Disable BSP compliance enforcement: the test fixture Nonce lacks an
        // EncodingType attribute (BSP:R4220). Both behaviours are spec-permitted;
        // we relax BSP enforcement on the lenient endpoint to mirror our server's
        // lenient authed SUT which does not enforce BSP-2005 Nonce attribute rules.
        lenientProps.put(WSHandlerConstants.IS_BSP_COMPLIANT, "false");

        WSS4JInInterceptor wss4jLenient = new WSS4JInInterceptor(lenientProps);
        sfSec.getInInterceptors().add(wss4jLenient);
        sfSec.create();
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soapsec (SOAP 1.2, WS-Sec lenient)");

        // WS-Security secured SOAP 1.2 endpoint — STRICT timestamp TTL (300 s default).
        // Rejects Created=2000-01-01 (stale by decades). Uses the same handler.
        JaxWsServerFactoryBean sfSecStrict = new JaxWsServerFactoryBean();
        sfSecStrict.setServiceClass(crossref.cxf.generated.ControlledPort.class);
        sfSecStrict.setServiceBean(new ControlledImpl());
        sfSecStrict.setBindingId(SOAPBinding.SOAP12HTTP_BINDING);
        sfSecStrict.setAddress("http://0.0.0.0:8082/soapsec-strict");

        Map<String, Object> strictProps = new HashMap<>();
        strictProps.put(WSHandlerConstants.ACTION, WSHandlerConstants.USERNAME_TOKEN);
        strictProps.put(WSHandlerConstants.PASSWORD_TYPE, "PasswordDigest");
        strictProps.put(WSHandlerConstants.PW_CALLBACK_CLASS, PasswordCallbackHandler.class.getName());
        // Strict: default TTL (300 s). The 2000-01-01 Created will be rejected as stale.
        // The 2020-01-01 Created is also years in the past, so it will also be rejected.
        // This endpoint is only used for wssec_stale_timestamp where BOTH should reject.
        strictProps.put(WSHandlerConstants.TTL_USERNAMETOKEN, "300");
        strictProps.put(WSHandlerConstants.TTL_FUTURE_USERNAMETOKEN, "300");
        // Disable BSP enforcement (same fixture structure as lenient endpoint).
        strictProps.put(WSHandlerConstants.IS_BSP_COMPLIANT, "false");

        WSS4JInInterceptor wss4jStrict = new WSS4JInInterceptor(strictProps);
        sfSecStrict.getInInterceptors().add(wss4jStrict);
        sfSecStrict.create();
        System.err.println("cxf-ref listening on 0.0.0.0:8082/soapsec-strict (SOAP 1.2, WS-Sec strict)");

        Thread.currentThread().join();
    }
}
