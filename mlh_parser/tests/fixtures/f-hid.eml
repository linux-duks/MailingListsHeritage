Received: from sendmail.purelymail.com (sendmail.purelymail.com [34.202.193.197])
	(using TLSv1.2 with cipher ECDHE-RSA-AES256-GCM-SHA384 (256/256 bits))
	(No client certificate requested)
	by smtp.subspace.kernel.org (Postfix) with ESMTPS id 2BA5C285CB9
	for <linux-input@vger.kernel.org>; Tue,  2 Jun 2026 03:05:37 +0000 (UTC)
Authentication-Results: smtp.subspace.kernel.org; arc=none smtp.client-ip=34.202.193.197
ARC-Seal:i=1; a=rsa-sha256; d=subspace.kernel.org; s=arc-20240116;
	t=1780369539; cv=none; b=m7zND6OfP7RSXhM6j36PvzJa6spQ2WFCnC6BAXyXu7feSLavLrf/41KUzz+TJ0eH8Sx2oW1hX3Bnf/gZ2zy5lWY4dODYdyAc2C2XYW06PA/GLLkpgLE08FggG9I4qafYlt3iKBzB6cFOZtiO4uJj2y18YaXzW5czBUz+VBgEq/c=
ARC-Message-Signature:i=1; a=rsa-sha256; d=subspace.kernel.org;
	s=arc-20240116; t=1780369539; c=relaxed/simple;
	bh=f3zVbPBAMrce9oSBZHsiqEpX15pcL+jBl7cA2FiUI4s=;
	h=From:To:Cc:Subject:Date:Message-ID:MIME-Version:Content-Type; b=YyZ0HBm7czcJHthsmV4rJMbf45YRPs8z8iLymxf1GaFIw/btkiaIgkUDnWH6lPgUqtClwccZG/KKBjKMGau3Zi2lwsLzyGa7JdQ0YBgFpZQYeRX+qIW0mbNFoyWh/MRzeNDuNwHwUQ59B+M489MOipIfLrVVz6h3ZEjN3xWK1uc=
ARC-Authentication-Results:i=1; smtp.subspace.kernel.org; dmarc=pass (p=quarantine dis=none) header.from=rcpassos.me; spf=pass smtp.mailfrom=rcpassos.me; dkim=pass (2048-bit key) header.d=rcpassos.me header.i=@rcpassos.me header.b=dZpNl4M/; dkim=pass (2048-bit key) header.d=purelymail.com header.i=@purelymail.com header.b=SpzwzfS+; arc=none smtp.client-ip=34.202.193.197
Authentication-Results: smtp.subspace.kernel.org; dmarc=pass (p=quarantine dis=none) header.from=rcpassos.me
Authentication-Results: smtp.subspace.kernel.org; spf=pass smtp.mailfrom=rcpassos.me
Authentication-Results: smtp.subspace.kernel.org;
	dkim=pass (2048-bit key) header.d=rcpassos.me header.i=@rcpassos.me header.b="dZpNl4M/";
	dkim=pass (2048-bit key) header.d=purelymail.com header.i=@purelymail.com header.b="SpzwzfS+"
Authentication-Results: purelymail.com; auth=pass
DKIM-Signature: a=rsa-sha256; b=dZpNl4M/vVRayhwY2CUX4NxA5wkkCt/bf+MEyySXB0jWmQT6eejyATPA90UFy/TkY9pi4ZfkbEExViyrWZ2+uaa3YQXFpbcRDH8L9xuAGgEX1oH2uyBWy5ynWD9jgbrSk20QjVfSokagagpxVEtcEpaajf0yoW/Rog4ipcSpfFZFCsDdHFROV35hx1+xOsRxfFf7Bze5HxPFUKkM5Niup3oq49O1UPDS1F2wqCwHM+BF9+WF5udDpWKAFQBumBnMUTthXVfOMjUOwsMq7SDT+oHh+swUc0Y+4vYhAGo7Jxr95OKqV1Iy7lzELQre4DiEuNbKPBjQn9MPqHN5Zgmpkg==; s=purelymail1; d=rcpassos.me; v=1; bh=f3zVbPBAMrce9oSBZHsiqEpX15pcL+jBl7cA2FiUI4s=; h=Received:From:To:Subject:Date;
DKIM-Signature: a=rsa-sha256; b=SpzwzfS+hiI/WR8j3bx7FQlYS1zNPs9aCD8QzW/D8WcLjQkjqoc5oO+ia0gUuRM1Xsoelv468fwFNsuwcEppv7+9sIzsYibu6V2op0fYoRo8RFBndeOpGy3UdTs7snxmXE2itJMMcXit1PBRY9swcwblYykw9zHInxq3xc4/dNtqNGQS3cPbT93qSfqJuE3i3pVODB5hf+a4/wiRM+xgb8xQqYqPaeIYvswnh9NaN5HjxOXAVu0l8EgzpPEAILUgyLkaZA4yBv+wSmAgqk/fpEUCKMZDqrGuMBb3lv6Z8WLImuf+wAVwSabD0UsqYtbYWWkpKVDHUufBuNgNEXtETA==; s=purelymail1; d=purelymail.com; v=1; bh=f3zVbPBAMrce9oSBZHsiqEpX15pcL+jBl7cA2FiUI4s=; h=Feedback-ID:Received:From:To:Subject:Date;
Feedback-ID: 45355:7809:null:purelymail
X-Pm-Original-To: linux-input@vger.kernel.org
Received: by smtp.purelymail.com (Purelymail SMTP) with ESMTPSA id -982909124;
          (version=TLSv1.3 cipher=TLS_AES_256_GCM_SHA384);
          Tue, 02 Jun 2026 03:05:35 +0000 (UTC)
From: Rafael Passos <rafael@rcpassos.me>
To: linux-input@vger.kernel.org,
	lcasmz54@gmail.com,
	bentiss@kernel.org
Cc: hadess@hadess.net,
	jikos@kernel.org,
	linux-kernel@vger.kernel.org,
	linux-pm@vger.kernel.org,
	sre@kernel.org,
	kernel@lists.ime.usp.br,
	Rafael Passos <rafael@rcpassos.me>
Subject: [PATCH v2] HID: Input: Add battery list cleanup with devm action
Date: Tue,  2 Jun 2026 00:05:19 -0300
Message-ID: <20260602030519.3097058-1-rafael@rcpassos.me>
X-Mailer: git-send-email 2.53.0
Precedence: bulk
X-Mailing-List: linux-input@vger.kernel.org
List-Id: <linux-input.vger.kernel.org>
List-Subscribe: <mailto:linux-input+subscribe@vger.kernel.org>
List-Unsubscribe: <mailto:linux-input+unsubscribe@vger.kernel.org>
MIME-Version: 1.0
Content-Transfer-Encoding: quoted-printable
X-MIME-Autoconverted: from 8bit to quoted-printable by Purelymail
Content-Type: text/plain; charset=UTF-8

The batteries list (hdev->batteries) is not cleaned up during
hidinput_disconnect(), but struct hid_battery entries are allocated
with devm_kzalloc.
When a driver is unbound (e.g. during devicereprobe), devm frees those
entries while their list_head nodesremain dangling in hdev->batteries,
which persists across rebinds.

Fixes: 4a58ae85c3f9 ("HID: input: Add support for multiple batteries per de=
vice")
Signed-off-by: Rafael Passos <rafael@rcpassos.me>
---
Hi, I hope there is time to land this in this cycle.
Tell me if I should make any changes.
I built and tested it againts the mainline.
I dont have devices with multiple batteries to test.

Bug report link:
    https://lore.kernel.org/all/20260602011949.2825852-1-rafael@rcpassos.me=
/

I received a sashiko review, and looked at wacom drivers for inspiration.

Changes in v2:
- removed list deletion from hidinput_disconnect
- new hidinput_cleanup_battery method
- register the cleanup call using devm action
  in hidinput_setup_battery

If my searching was corect, this shoul be called when devres releases its g=
roup
The commit message changed, because it referenced the
hidinput_disconnect method.

Original patch: https://lore.kernel.org/linux-input/20260602020352.4256D1F0=
0893@smtp.kernel.org/T/#t
The sashiko review: https://sashiko.dev/#/patchset/20260602015029.2838058-1=
-rafael@rcpassos.me?part=3D1

 drivers/hid/hid-input.c | 13 +++++++++++++
 1 file changed, 13 insertions(+)

diff --git a/drivers/hid/hid-input.c b/drivers/hid/hid-input.c
index d73cfa2e73d3..c7b8c4ff7a33 100644
--- a/drivers/hid/hid-input.c
+++ b/drivers/hid/hid-input.c
@@ -519,6 +519,13 @@ static struct hid_battery *hidinput_find_battery(struc=
t hid_device *dev,
 =09return NULL;
 }
=20
+static void hidinput_cleanup_battery(void *res)
+{
+=09struct hid_battery *bat =3D res;
+
+=09list_del(&bat->list);
+}
+
 static int hidinput_setup_battery(struct hid_device *dev, unsigned report_=
type,
 =09=09=09=09  struct hid_field *field, bool is_percentage)
 {
@@ -610,6 +617,12 @@ static int hidinput_setup_battery(struct hid_device *d=
ev, unsigned report_type,
=20
 =09power_supply_powers(bat->ps, &dev->dev);
 =09list_add_tail(&bat->list, &dev->batteries);
+
+=09error =3D devm_add_action_or_reset(&dev->dev,
+=09=09=09=09=09 hidinput_cleanup_battery, bat);
+=09if (error)
+=09=09return error;
+
 =09return 0;
=20
 err_free_name:
--=20
2.53.0

