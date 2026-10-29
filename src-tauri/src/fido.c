#include <fido.h>
#include <fido/param.h>
#include <string.h>
#include <stdlib.h>
/* A narrow, in-process libfido2 adapter. Secrets never enter argv or a file. */
int elo_fido(int enroll, const unsigned char *challenge, const unsigned char *salt,
             const unsigned char *user, unsigned char *credential, size_t *credential_len,
             const char *pin, unsigned char *secret) {
    int result = FIDO_ERR_NOTFOUND;
    size_t count = 0;
    fido_init(0);
    fido_dev_info_t *list = fido_dev_info_new(8);
    fido_dev_t *dev = NULL;
    fido_cred_t *cred = NULL;
    fido_assert_t *assertion = NULL;
    if (!list || fido_dev_info_manifest(list, 8, &count) != FIDO_OK || count != 1) goto done;
    dev = fido_dev_new();
    if (!dev || (result = fido_dev_open(dev, fido_dev_info_path(fido_dev_info_ptr(list, 0)))) != FIDO_OK) goto done;
    fido_dev_set_timeout(dev, 30000);
    if (enroll) {
        cred = fido_cred_new();
        if (!cred) { result = FIDO_ERR_INTERNAL; goto done; }
        if ((result = fido_cred_set_type(cred, COSE_ES256)) != FIDO_OK ||
            (result = fido_cred_set_clientdata_hash(cred, challenge, 32)) != FIDO_OK ||
            (result = fido_cred_set_rp(cred, "elo-talks.local", "Elo Talks")) != FIDO_OK ||
            (result = fido_cred_set_user(cred, user, 32, "vault", "Elo Talks", NULL)) != FIDO_OK ||
            (result = fido_cred_set_extensions(cred, FIDO_EXT_HMAC_SECRET)) != FIDO_OK ||
            (result = fido_cred_set_uv(cred, FIDO_OPT_TRUE)) != FIDO_OK ||
            (result = fido_dev_make_cred(dev, cred, pin && *pin ? pin : NULL)) != FIDO_OK) goto done;
        if (fido_cred_id_len(cred) == 0 || fido_cred_id_len(cred) > *credential_len) { result = FIDO_ERR_INVALID_ARGUMENT; goto done; }
        *credential_len = fido_cred_id_len(cred);
        memcpy(credential, fido_cred_id_ptr(cred), *credential_len);
    }
    assertion = fido_assert_new();
    if (!assertion) { result = FIDO_ERR_INTERNAL; goto done; }
    if ((result = fido_assert_set_rp(assertion, "elo-talks.local")) != FIDO_OK ||
        (result = fido_assert_set_clientdata_hash(assertion, challenge, 32)) != FIDO_OK ||
        (result = fido_assert_allow_cred(assertion, credential, *credential_len)) != FIDO_OK ||
        (result = fido_assert_set_extensions(assertion, FIDO_EXT_HMAC_SECRET)) != FIDO_OK ||
        (result = fido_assert_set_hmac_salt(assertion, salt, 32)) != FIDO_OK ||
        (result = fido_assert_set_up(assertion, FIDO_OPT_TRUE)) != FIDO_OK ||
        (result = fido_assert_set_uv(assertion, FIDO_OPT_TRUE)) != FIDO_OK ||
        (result = fido_dev_get_assert(dev, assertion, pin && *pin ? pin : NULL)) != FIDO_OK) goto done;
    if (fido_assert_hmac_secret_len(assertion, 0) != 32) { result = FIDO_ERR_UNSUPPORTED_EXTENSION; goto done; }
    memcpy(secret, fido_assert_hmac_secret_ptr(assertion, 0), 32);
    result = FIDO_OK;
done:
    if (assertion) fido_assert_free(&assertion);
    if (cred) fido_cred_free(&cred);
    if (dev) { fido_dev_close(dev); fido_dev_free(&dev); }
    if (list) fido_dev_info_free(&list, 8);
    return result;
}
