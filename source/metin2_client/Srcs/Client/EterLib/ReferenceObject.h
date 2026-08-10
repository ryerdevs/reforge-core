#pragma once

class CReferenceObject
{
	public:
		CReferenceObject();
		virtual ~CReferenceObject();

		void AddReference();
		void AddReferenceOnly();
		void Release();

		int GetReferenceCount() const;

		bool canDestroy() const;

	protected:
		virtual void OnConstruct();
		virtual void OnSelfDestruct();

	private:
		int m_refCount;
		bool m_destructed;
};

