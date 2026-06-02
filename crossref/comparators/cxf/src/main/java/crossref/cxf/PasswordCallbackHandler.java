package crossref.cxf;

import org.apache.wss4j.common.ext.WSPasswordCallback;

import javax.security.auth.callback.Callback;
import javax.security.auth.callback.CallbackHandler;
import javax.security.auth.callback.UnsupportedCallbackException;
import java.io.IOException;

/**
 * WSS4J callback handler: supplies the plaintext password for known users so
 * WSS4J can verify PasswordDigest tokens itself.
 * Only "alice" is recognised; all others result in no password being set,
 * which causes WSS4J to reject the token.
 */
public class PasswordCallbackHandler implements CallbackHandler {

    @Override
    public void handle(Callback[] callbacks) throws IOException, UnsupportedCallbackException {
        for (Callback cb : callbacks) {
            if (cb instanceof WSPasswordCallback wpc) {
                if ("alice".equals(wpc.getIdentifier())) {
                    wpc.setPassword("secret");
                }
                // Unknown user: leave password unset → WSS4J rejects the token.
            }
        }
    }
}
