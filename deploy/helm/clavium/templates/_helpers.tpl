{{- define "clavium.name" -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "clavium.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "clavium.labels" -}}
app.kubernetes.io/name: {{ include "clavium.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "clavium.selectorLabels" -}}
app.kubernetes.io/name: {{ include "clavium.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "clavium.apiKeysNamespace" -}}
{{- default .Release.Namespace .Values.apiKeysNamespace -}}
{{- end -}}

{{- define "clavium.cookieSecretName" -}}
{{- default (printf "%s-cookie-key" (include "clavium.fullname" .)) .Values.cookieKey.existingSecret -}}
{{- end -}}
