UPDATE app_settings
SET document = json_remove(json_set(document, '$.instance', json_extract(document, '$.feed')), '$.feed')
WHERE json_type(document, '$.feed') = 'object';
