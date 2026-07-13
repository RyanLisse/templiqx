{{- define "templiqx.name" -}}
templiqx
{{- end -}}

{{- define "templiqx.fullname" -}}
{{ .Release.Name }}-templiqx
{{- end -}}

{{- define "templiqx.image" -}}
{{- $digest := default "" .Values.image.digest -}}
{{- if $digest -}}
{{- if not (regexMatch "^sha256:[a-f0-9]{64}$" $digest) -}}
{{- fail "image.digest must be an OCI sha256 digest" -}}
{{- end -}}
{{- printf "%s@%s" .Values.image.repository $digest -}}
{{- else -}}
{{- printf "%s:%s" .Values.image.repository .Values.image.tag -}}
{{- end -}}
{{- end -}}
